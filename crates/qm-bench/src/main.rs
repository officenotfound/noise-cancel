//! Offline benchmark for QuietMic denoisers: speed, delay and (with a clean reference) SI-SDR.
//!
//! qm-bench --model dfn3-ll --in noisy.wav --out clean.wav [--ref reference.wav] [--atten-lim 30]

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use qm_dsp::{Denoiser, DfnDenoiser, FastEnhancer, RnnoiseDenoiser, SAMPLE_RATE};

/// Frames excluded from the timing stats while the runtime warms up.
const WARMUP_FRAMES: usize = 20;
/// Largest delay (samples) searched for when measuring end-to-end delay.
const MAX_LAG: usize = 4_800;

#[derive(Copy, Clone, ValueEnum)]
enum Model {
    Dfn3,
    Dfn3Ll,
    Rnnoise,
    FeT,
    FeB,
    FeS,
    FeM,
    FeL,
}

#[derive(Parser)]
struct Args {
    #[arg(long, value_enum)]
    model: Model,
    #[arg(long = "in")]
    input: PathBuf,
    #[arg(long)]
    out: Option<PathBuf>,
    /// Clean reference (same length as the input, aligned) for SI-SDR.
    #[arg(long = "ref")]
    reference: Option<PathBuf>,
    /// DeepFilterNet attenuation limit in dB (lower keeps more of the noise).
    #[arg(long)]
    atten_lim: Option<f32>,
    #[arg(long, default_value = "models")]
    models_dir: PathBuf,
    /// Write the output without removing the model delay.
    #[arg(long)]
    keep_delay: bool,
    /// Print one JSON object instead of the human-readable report.
    #[arg(long)]
    json: bool,
}

fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("opening {}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE {
        bail!("{}: expected {} Hz, got {} Hz (resample first, e.g. with ffmpeg -ar 48000)", path.display(), SAMPLE_RATE, spec.sample_rate);
    }
    let ch = spec.channels as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    Ok(interleaved.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect())
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn build(model: Model, args: &Args) -> Result<Box<dyn Denoiser>> {
    Ok(match model {
        Model::Dfn3 => Box::new(DfnDenoiser::new("dfn3", &args.models_dir.join("DeepFilterNet3_onnx.tar.gz"), args.atten_lim)?),
        Model::Dfn3Ll => Box::new(DfnDenoiser::new("dfn3-ll", &args.models_dir.join("DeepFilterNet3_ll_onnx.tar.gz"), args.atten_lim)?),
        Model::Rnnoise => Box::new(RnnoiseDenoiser::new()),
        Model::FeT => fe("t", args)?,
        Model::FeB => fe("b", args)?,
        Model::FeS => fe("s", args)?,
        Model::FeM => fe("m", args)?,
        Model::FeL => fe("l", args)?,
    })
}

fn fe(size: &str, args: &Args) -> Result<Box<dyn Denoiser>> {
    let path = args.models_dir.join(format!("fastenhancer/fastenhancer_{size}_spec.onnx"));
    Ok(Box::new(FastEnhancer::new(size, &path)?))
}

/// Lag (samples) at which `b` best lines up with `a` (b[n + lag] ~ a[n]), over 0..=MAX_LAG.
fn best_lag(a: &[f32], b: &[f32]) -> usize {
    let n = a.len().min(b.len()).saturating_sub(MAX_LAG).min(48_000 * 20);
    // Skip the first second: model warm-up can distort the start.
    let start = 48_000.min(n / 4);
    let mut best = (0usize, f64::MIN);
    for lag in 0..=MAX_LAG {
        let c: f64 = (start..n).map(|i| a[i] as f64 * b[i + lag] as f64).sum();
        if c > best.1 {
            best = (lag, c);
        }
    }
    best.0
}

/// Scale-invariant SDR in dB of `est` against `reference` (equal length).
fn si_sdr(reference: &[f32], est: &[f32]) -> f64 {
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum::<f64>();
    let alpha = dot(est, reference) / (dot(reference, reference) + 1e-12);
    let (mut target, mut noise) = (0.0f64, 0.0f64);
    for (r, e) in reference.iter().zip(est) {
        let t = alpha * *r as f64;
        target += t * t;
        let d = *e as f64 - t;
        noise += d * d;
    }
    10.0 * ((target + 1e-12) / (noise + 1e-12)).log10()
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[(((sorted.len() - 1) as f64) * p).round() as usize]
}

fn main() -> Result<()> {
    let args = Args::parse();
    let noisy = read_wav(&args.input)?;
    let mut dn = build(args.model, &args)?;
    let delay = dn.algorithmic_delay();
    let hop = dn.hop();

    // Pad the tail so the delayed output is complete, and to a whole number of frames.
    let total = (noisy.len() + delay).div_ceil(hop) * hop;
    let mut padded = noisy.clone();
    padded.resize(total, 0.0);
    let mut out = vec![0.0f32; total];

    let mut frame_ms = Vec::with_capacity(total / hop);
    let wall = Instant::now();
    for (i, (inp, outp)) in padded.chunks_exact(hop).zip(out.chunks_exact_mut(hop)).enumerate() {
        let t = Instant::now();
        dn.process_frame(inp, outp)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        if i >= WARMUP_FRAMES {
            frame_ms.push(ms);
        }
    }
    let wall_s = wall.elapsed().as_secs_f64();
    let audio_s = total as f64 / SAMPLE_RATE as f64;
    frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = frame_ms.iter().sum::<f64>() / frame_ms.len() as f64;
    // A call that takes longer than the audio it covers is an underrun in a live pipeline.
    let hop_ms = hop as f64 * 1e3 / SAMPLE_RATE as f64;
    let over_hop = frame_ms.iter().filter(|&&m| m > hop_ms).count();

    // Measured delay: align the output to the clean reference if we have one, else to the noisy input.
    let reference = args.reference.as_deref().map(read_wav).transpose()?;
    let measured = best_lag(reference.as_deref().unwrap_or(&noisy), &out);

    // Remove the measured delay so listening copies and metrics line up with the input.
    let aligned: Vec<f32> = out.iter().skip(measured).take(noisy.len()).copied().collect();
    let (sdr_in, sdr_out) = match &reference {
        Some(r) => {
            let n = r.len().min(aligned.len()).min(noisy.len());
            (Some(si_sdr(&r[..n], &noisy[..n])), Some(si_sdr(&r[..n], &aligned[..n])))
        }
        None => (None, None),
    };

    if let Some(path) = &args.out {
        if args.keep_delay {
            write_wav(path, &out)?;
        } else {
            write_wav(path, &aligned)?;
        }
    }

    let ms = |s: usize| s as f64 * 1e3 / SAMPLE_RATE as f64;
    let report = serde_json::json!({
        "model": dn.name(),
        "file": args.input.file_name().map(|f| f.to_string_lossy()),
        "audio_s": audio_s,
        "rtf": wall_s / audio_s,
        "frame_ms_mean": mean,
        "frame_ms_p50": percentile(&frame_ms, 0.50),
        "frame_ms_p99": percentile(&frame_ms, 0.99),
        "frame_ms_max": frame_ms.last(),
        "hop_ms": hop_ms,
        "frames_over_10ms": over_hop,
        "frames_over_hop": over_hop,
        "algorithmic_delay_ms": ms(delay),
        "measured_delay_ms": ms(measured),
        "si_sdr_in_db": sdr_in,
        "si_sdr_out_db": sdr_out,
        "si_sdr_gain_db": sdr_in.zip(sdr_out).map(|(a, b)| b - a),
    });
    if args.json {
        println!("{report}");
    } else {
        println!("{:<10} {}", dn.name(), args.input.display());
        println!("  speed   : RTF {:.3} ({:.1}% of one core at real time) | frame mean {:.2} ms, p50 {:.2}, p99 {:.2}, max {:.2} | {} frames > hop ({:.1} ms)", wall_s / audio_s, 100.0 * wall_s / audio_s, mean, percentile(&frame_ms, 0.5), percentile(&frame_ms, 0.99), frame_ms.last().unwrap(), over_hop, hop_ms);
        println!("  delay   : algorithmic {:.1} ms, measured {:.1} ms", ms(delay), ms(measured));
        if let (Some(a), Some(b)) = (sdr_in, sdr_out) {
            println!("  SI-SDR  : {a:.1} dB -> {b:.1} dB ({:+.1} dB)", b - a);
        }
    }
    Ok(())
}
