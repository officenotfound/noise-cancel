//! Offline accuracy test for speaker embeddings.
//!
//! eval_embed --model models/voice/wespeaker_en_voxceleb_resnet34.onnx --data testdata/voice16k [--enroll 30] [--cap 60]
//!
//! For every speaker: enroll on their first `--enroll` seconds, then score held-out windows (1, 1.5, 2, 3 s) from that
//! speaker (should match) and from every other speaker (should not). Reports equal-error rate and, for fixed rates
//! of wrongly rejecting the owner, how often other voices are wrongly accepted.

use std::path::{Path, PathBuf};
use std::time::Instant;

use qm_voice::wav::read_wav;
use qm_voice::{cosine, l2_normalize, SpeakerEmbedder};

const SR: usize = 16_000;
const WINDOWS_S: [f32; 4] = [1.0, 1.5, 2.0, 3.0];

struct Speaker {
    id: String,
    enroll: Vec<f32>,
    holdout: Vec<Vec<f32>>, // utterances
}

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| default.to_string())
}

fn load_speakers(dir: &Path, enroll_s: f32) -> Vec<Speaker> {
    let mut spk: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    spk.sort();
    let mut out = Vec::new();
    for d in spk {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "wav")).collect();
        files.sort();
        let mut enroll = Vec::new();
        let mut holdout = Vec::new();
        for f in files {
            let (x, sr) = read_wav(&f).unwrap();
            assert_eq!(sr as usize, SR);
            if enroll.len() < (enroll_s * SR as f32) as usize {
                enroll.extend(x);
            } else {
                holdout.push(x);
            }
        }
        enroll.truncate((enroll_s * SR as f32) as usize);
        let hold_s: usize = holdout.iter().map(|u| u.len()).sum::<usize>() / SR;
        if enroll.len() >= (enroll_s * SR as f32) as usize && hold_s >= 30 {
            out.push(Speaker { id: d.file_name().unwrap().to_string_lossy().into(), enroll, holdout });
        }
    }
    out
}

/// Equal-error rate and the false-accept rate at fixed false-reject rates. `pos` = owner scores, `neg` = others.
fn metrics(pos: &mut Vec<f32>, neg: &mut Vec<f32>) -> (f32, f32, Vec<(f32, f32, f32)>) {
    pos.sort_by(|a, b| a.partial_cmp(b).unwrap());
    neg.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let frr = |t: f32| pos.partition_point(|s| *s < t) as f32 / pos.len() as f32;
    let far = |t: f32| (neg.len() - neg.partition_point(|s| *s < t)) as f32 / neg.len() as f32;
    // EER: sweep thresholds over all scores
    let mut eer = (1.0f32, 0.0f32);
    for t in pos.iter().chain(neg.iter()) {
        let (a, b) = (frr(*t), far(*t));
        if (a - b).abs() < (eer.0 - eer.1).abs() {
            eer = (a, b);
        }
    }
    let at = [0.01f32, 0.02, 0.05, 0.10].iter().map(|target| {
        let idx = ((pos.len() as f32 * target) as usize).min(pos.len() - 1);
        let t = pos[idx]; // threshold that rejects `target` of the owner's windows
        (*target, t, far(t))
    }).collect();
    ((eer.0 + eer.1) / 2.0, pos.iter().sum::<f32>() / pos.len() as f32 - neg.iter().sum::<f32>() / neg.len() as f32, at)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let model = arg(&args, "--model", "");
    let data = arg(&args, "--data", "testdata/voice16k");
    let enroll_s: f32 = arg(&args, "--enroll", "30").parse()?;
    let cap: usize = arg(&args, "--cap", "60").parse()?;

    let mut emb = SpeakerEmbedder::new(Path::new(&model))?;
    let speakers = load_speakers(Path::new(&data), enroll_s);
    println!("model {} (dim {}), {} speakers, enrollment {} s, up to {} windows per speaker and length", model, emb.dim, speakers.len(), enroll_s, cap);

    // enrollment embeddings: mean of normalised 4 s chunks
    let t0 = Instant::now();
    let mut enroll_emb: Vec<Vec<f32>> = Vec::new();
    for s in &speakers {
        let mut sum = vec![0.0f32; emb.dim];
        for chunk in s.enroll.chunks_exact(4 * SR) {
            let e = emb.embed(chunk)?;
            sum.iter_mut().zip(&e).for_each(|(a, b)| *a += b);
        }
        l2_normalize(&mut sum);
        enroll_emb.push(sum);
    }
    println!("enrollment done in {:.1}s", t0.elapsed().as_secs_f32());

    println!("\n{:>6} | {:>7} {:>9} | false-accept of OTHER voices when the owner is wrongly rejected 1% / 2% / 5% / 10% of windows", "window", "EER", "score gap");
    let mut ms_per_embed = 0.0;
    for &l in &WINDOWS_S {
        let n = (l * SR as f32) as usize;
        // windows per speaker (non-overlapping inside utterances, evenly subsampled)
        let t1 = Instant::now();
        let mut count = 0usize;
        let mut scores: Vec<Vec<f32>> = vec![Vec::new(); speakers.len()]; // scores[w_speaker] = per target
        let mut per_window: Vec<(usize, Vec<f32>)> = Vec::new();
        for (si, s) in speakers.iter().enumerate() {
            let mut wins: Vec<&[f32]> = Vec::new();
            for u in &s.holdout {
                wins.extend(u.chunks_exact(n));
            }
            let step = (wins.len() as f32 / cap as f32).max(1.0);
            let mut i = 0.0f32;
            while (i as usize) < wins.len() {
                let e = emb.embed(wins[i as usize])?;
                per_window.push((si, enroll_emb.iter().map(|t| cosine(&e, t)).collect()));
                count += 1;
                i += step;
            }
        }
        ms_per_embed = t1.elapsed().as_secs_f32() * 1000.0 / count as f32;
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for (si, sc) in &per_window {
            for (ti, s) in sc.iter().enumerate() {
                if ti == *si { pos.push(*s) } else { neg.push(*s) }
            }
        }
        let _ = &mut scores;
        let (eer, gap, at) = metrics(&mut pos, &mut neg);
        let cells: Vec<String> = at.iter().map(|(_, t, far)| format!("{:>5.1}% (thr {:.2})", far * 100.0, t)).collect();
        println!("{:>5.1}s | {:>6.2}% {:>9.2} | {}", l, eer * 100.0, gap, cells.join("  "));
    }
    println!("\nabout {ms_per_embed:.0} ms per embedding (single thread, release build)");
    Ok(())
}
