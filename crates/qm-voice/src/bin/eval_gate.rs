//! Simulate the voice-print gate on 60 s streams where the owner and other speakers alternate.
//!
//! eval_gate --embedder models/voice/wespeaker_en_voxceleb_CAM++.onnx --vad models/voice/silero_vad_16k_sequence.onnx
//!           --data testdata/voice16k [--targets 8] [--streams 3]
//!
//! The gate is causal (no look-ahead), starts open, re-opens after every pause (so a speaker's first words are never
//! clipped), and closes only after the embedding scores of the recent window stay below a threshold. Scores are
//! computed once per stream; the sweep then re-runs only the cheap decision logic.

use std::path::{Path, PathBuf};

use qm_voice::vad::CHUNK;
use qm_voice::wav::{read_wav, write_wav};
use qm_voice::{cosine, l2_normalize, SpeakerEmbedder, Vad};

const SR: usize = 16_000;
const HOP_S: f32 = 0.25;
const WINDOWS_S: [f32; 3] = [1.0, 1.5, 2.0];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n - 1)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Who {
    Silence,
    Owner,
    Other,
}

struct Speaker {
    enroll: Vec<f32>,
    utts: Vec<Vec<f32>>, // hold-out utterances
}

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| default.to_string())
}

fn load(dir: &Path) -> Vec<Speaker> {
    let mut spk: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    spk.sort();
    let mut out = Vec::new();
    for d in spk {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "wav")).collect();
        files.sort();
        let (mut enroll, mut utts) = (Vec::new(), Vec::new());
        for f in files {
            let (x, _) = read_wav(&f).unwrap();
            if enroll.len() < 30 * SR {
                enroll.extend(x);
            } else if x.len() < 12 * SR {
                utts.push(x);
            }
        }
        enroll.truncate(30 * SR);
        if enroll.len() == 30 * SR && utts.len() >= 8 {
            out.push(Speaker { enroll, utts });
        }
    }
    out
}

struct Stream {
    audio: Vec<f32>,
    who: Vec<Who>,        // per 32 ms chunk (ground truth, speech spans only)
    voiced: Vec<bool>,    // per chunk: energetic enough to be actual speech (so pauses inside utterances do not count)
    vad: Vec<f32>,        // per chunk speech probability
    scores: Vec<Vec<f32>>, // per window length: score at each hop (NaN when too little speech)
}

fn build_stream(speakers: &[Speaker], target: usize, rng: &mut Rng, secs: f32) -> (Vec<f32>, Vec<Who>) {
    let mut audio = Vec::new();
    let mut who_samples: Vec<Who> = Vec::new();
    while audio.len() < (secs * SR as f32) as usize {
        let gap = (rng.range(0.4, 1.5) * SR as f32) as usize;
        audio.extend(vec![0.0; gap]);
        who_samples.extend(vec![Who::Silence; gap]);
        let owner = rng.next() < 0.5;
        let s = if owner { target } else { loop { let o = rng.pick(speakers.len()); if o != target { break o } } };
        let u = &speakers[s].utts[rng.pick(speakers[s].utts.len())];
        audio.extend_from_slice(u);
        who_samples.extend(vec![if owner { Who::Owner } else { Who::Other }; u.len()]);
    }
    let chunks = audio.len() / CHUNK;
    let who = (0..chunks).map(|c| who_samples[c * CHUNK + CHUNK / 2]).collect();
    (audio, who)
}

fn precompute(audio: Vec<f32>, emb_audio: Vec<f32>, who: Vec<Who>, vad: &mut Vad, emb: &mut SpeakerEmbedder, enroll: &[f32]) -> Stream {
    vad.reset();
    let chunks = audio.len() / CHUNK;
    let probs: Vec<f32> = (0..chunks).map(|c| vad.process(&audio[c * CHUNK..(c + 1) * CHUNK]).unwrap()).collect();
    // "voiced" truth: inside a speech span and louder than 30% of the span's typical level
    let rms: Vec<f32> = (0..chunks).map(|c| (audio[c * CHUNK..(c + 1) * CHUNK].iter().map(|v| v * v).sum::<f32>() / CHUNK as f32).sqrt()).collect();
    let mut sorted: Vec<f32> = rms.iter().zip(&who).filter(|(_, w)| **w != Who::Silence).map(|(r, _)| *r).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let typical = sorted[sorted.len() * 7 / 10];
    let voiced = (0..chunks).map(|c| who[c] != Who::Silence && rms[c] > 0.3 * typical).collect();

    let hops = (audio.len() as f32 / SR as f32 / HOP_S) as usize;
    let mut scores = Vec::new();
    for &w in &WINDOWS_S {
        let n = (w * SR as f32) as usize;
        let mut v = vec![f32::NAN; hops + 1];
        for k in 0..=hops {
            let end = (k as f32 * HOP_S * SR as f32) as usize;
            if end < n || end > audio.len() {
                continue;
            }
            // only embed windows that are mostly speech
            let (c0, c1) = ((end - n) / CHUNK, (end / CHUNK).min(chunks));
            let speechy = probs[c0..c1].iter().filter(|p| **p > 0.5).count() as f32 / (c1 - c0) as f32;
            if speechy >= 0.5 {
                let e = emb.embed(&emb_audio[end - n..end]).unwrap();
                v[k] = cosine(&e, enroll);
            }
        }
        scores.push(v);
    }
    Stream { audio, who, voiced, vad: probs, scores }
}

#[derive(Default, Clone)]
struct Tally {
    owner_voiced: f32,
    owner_muted: f32,
    onset_muted: f32,
    owner_segments: f32,
    other_voiced: f32,
    other_passed: f32,
    other_segments: f32,
    leak_s: f32,
}

fn run_gate(s: &Stream, wi: usize, t_close: f32, t_open: f32, close_after: usize, pause_s: f32, tally: &mut Tally) {
    let chunk_s = CHUNK as f32 / SR as f32;
    let hop_chunks = (HOP_S / chunk_s).round() as usize;
    let chunks = s.vad.len();
    let (mut open, mut below, mut last_speech) = (true, 0usize, -10.0f32);
    let mut gate = vec![true; chunks];
    for c in 0..chunks {
        let t = c as f32 * chunk_s;
        if s.vad[c] > 0.5 {
            last_speech = t;
        }
        if t - last_speech >= pause_s && s.vad[c] <= 0.5 {
            open = true;
            below = 0;
        }
        // a new decision every hop
        if c % hop_chunks == 0 {
            let k = (c as f32 * chunk_s / HOP_S).round() as usize;
            let sc = s.scores[wi].get(k).copied().unwrap_or(f32::NAN);
            if !sc.is_nan() {
                if sc < t_close {
                    below += 1;
                } else {
                    below = 0;
                    if sc >= t_open {
                        open = true;
                    }
                }
                if below >= close_after {
                    open = false;
                }
            }
        }
        gate[c] = open;
    }
    // tally per ground-truth segment
    let mut c = 0;
    while c < chunks {
        let w = s.who[c];
        if w == Who::Silence {
            c += 1;
            continue;
        }
        let start = c;
        while c < chunks && s.who[c] == w {
            c += 1;
        }
        let seg_start_t = start as f32 * chunk_s;
        let mut leak_until = 0.0f32;
        for i in start..c {
            if !s.voiced[i] {
                continue;
            }
            let rel = i as f32 * chunk_s - seg_start_t;
            match w {
                Who::Owner => {
                    tally.owner_voiced += chunk_s;
                    if !gate[i] {
                        tally.owner_muted += chunk_s;
                        if rel < 0.5 {
                            tally.onset_muted += chunk_s;
                        }
                    }
                }
                Who::Other => {
                    tally.other_voiced += chunk_s;
                    if gate[i] {
                        tally.other_passed += chunk_s;
                        leak_until = rel;
                    }
                }
                Who::Silence => {}
            }
        }
        if w == Who::Owner { tally.owner_segments += 1.0 } else { tally.other_segments += 1.0; tally.leak_s += leak_until }
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mut emb = SpeakerEmbedder::new(Path::new(&arg(&a, "--embedder", "")))?;
    let mut vad = Vad::new(Path::new(&arg(&a, "--vad", ""))).unwrap();
    let speakers = load(Path::new(&arg(&a, "--data", "testdata/voice16k")));
    let (targets, per): (usize, usize) = (arg(&a, "--targets", "8").parse()?, arg(&a, "--streams", "3").parse()?);
    println!("{} speakers usable; {} targets x {} streams of 60 s", speakers.len(), targets, per);

    let mut rng = Rng(12345);
    let mut streams: Vec<Stream> = Vec::new();
    let (dump, load_dir) = (a.iter().position(|x| x == "--dump").map(|i| a[i + 1].clone()), a.iter().position(|x| x == "--load").map(|i| a[i + 1].clone()));
    let mut dumped = 0usize;
    let mut skipped = 0usize;
    let only_noise: Option<usize> = a.iter().position(|x| x == "--only-noise").map(|i| a[i + 1].parse().unwrap());
    for t in 0..targets.min(speakers.len()) {
        let mut enroll_emb = vec![0.0f32; emb.dim];
        let mut enroll = speakers[t].enroll.clone();
        if let Some(d) = &dump {
            write_wav(&Path::new(d).join(format!("e{t}.wav")), &enroll)?;
        }
        if let (Some(d), true) = (&load_dir, a.iter().any(|x| x == "--enroll-denoised")) {
            enroll = read_wav(&Path::new(d).join(format!("pe{t}.wav")))?.0;
            enroll.resize(30 * SR, 0.0);
        }
        let mut parts = vec![enroll.clone()];
        if let (Some(d), true) = (&load_dir, a.iter().any(|x| x == "--enroll-aug")) {
            for k in 0..6 {
                let mut x = read_wav(&Path::new(d).join(format!("ea{t}_{k}.wav")))?.0;
                x.resize(30 * SR, 0.0);
                parts.push(x);
            }
        }
        for chunk in parts.iter().flat_map(|e| e.chunks_exact(4 * SR)) {
            let e = emb.embed(chunk)?;
            enroll_emb.iter_mut().zip(&e).for_each(|(x, y)| *x += y);
        }
        l2_normalize(&mut enroll_emb);
        for _ in 0..per {
            let (mut audio, who) = build_stream(&speakers, t, &mut rng, 60.0);
            let idx = streams.len() + dumped + skipped;
            if let Some(k) = only_noise {
                if idx % 6 != k {
                    skipped += 1;
                    continue;
                }
            }
            if let Some(d) = &dump {
                write_wav(&Path::new(d).join(format!("s{idx}.wav")), &audio)?;
                dumped += 1;
                continue;
            }
            let mut emb_audio = Vec::new();
            if let Some(d) = &load_dir {
                let (x, _) = read_wav(&Path::new(d).join(format!("p{idx}.wav")))?;
                let n = audio.len();
                audio = x;
                audio.resize(n, 0.0);
                if a.iter().any(|x| x == "--embed-raw") {
                    emb_audio = read_wav(&Path::new(d).join(format!("r{idx}.wav")))?.0;
                    emb_audio.resize(n, 0.0);
                }
            }
            if emb_audio.is_empty() {
                emb_audio = audio.clone();
            }
            streams.push(precompute(audio, emb_audio, who, &mut vad, &mut emb, &enroll_emb));
        }
        eprintln!("target {t} done");
    }

    if dump.is_some() {
        println!("dumped {dumped} streams");
        return Ok(());
    }
    let mut rows = Vec::new();
    for (wi, &w) in WINDOWS_S.iter().enumerate() {
        for pause in [0.3f32, 0.5, 0.8, 1.2] {
            for tc in [0.30f32, 0.40, 0.50] {
                for n in [1usize, 2] {
                    let mut t = Tally::default();
                    for s in &streams {
                        run_gate(s, wi, tc, tc + 0.10, n, pause, &mut t);
                    }
                    rows.push((w, pause, tc, n, 100.0 * t.owner_muted / t.owner_voiced, t.onset_muted / t.owner_segments, 100.0 * t.other_passed / t.other_voiced, t.leak_s / t.other_segments));
                }
            }
        }
    }
    println!("{:>5} {:>6} {:>7} {:>3} | {:>10} {:>14} | {:>12} {:>12}", "win", "pause", "t_close", "n", "own muted", "own onset 0.5s", "others pass", "leak/segment");
    for (w, pause, tc, n, om, onset, op, leak) in &rows {
        println!("{:>4.1}s {:>5.1}s {:>7.2} {:>3} | {:>9.1}% {:>13.2}s | {:>11.1}% {:>11.1}s", w, pause, tc, n, om, onset, op, leak);
    }
    let _ = streams.iter().map(|s| s.audio.len()).sum::<usize>();
    Ok(())
}
