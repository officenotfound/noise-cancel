//! `qm`: hear and inspect QuietMic presets without the Windows app.
//!
//!   qm presets                                   list presets, ladders and gate settings
//!   qm calibrate --preset auto                   time the preset's models on this PC and show the pick
//!   qm process --preset low-latency in.wav out.wav [--model fe-s] [--block 480] [--no-gate]

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use qm_dsp::SAMPLE_RATE;
use qm_engine::governor::{calibrate, pick};
use qm_engine::{Config, Engine, ModelId, Preset};

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "models", global = true)]
    models_dir: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Presets,
    Calibrate {
        #[arg(long, default_value = "auto")]
        preset: Preset,
        #[arg(long, default_value_t = 4.0)]
        seconds: f32,
    },
    Process {
        #[arg(long, default_value = "auto")]
        preset: Preset,
        input: PathBuf,
        output: PathBuf,
        /// Force a model instead of the preset's first ladder entry.
        #[arg(long)]
        model: Option<ModelId>,
        /// Simulated audio callback size in samples.
        #[arg(long, default_value_t = 480)]
        block: usize,
        #[arg(long)]
        no_gate: bool,
        /// Override the gate threshold (dB below the learned speech level).
        #[arg(long)]
        gate_threshold: Option<f32>,
        #[arg(long)]
        gate_ratio: Option<f32>,
        #[arg(long)]
        gate_max: Option<f32>,
        #[arg(long)]
        gate_hold: Option<f32>,
        /// Keep the total delay in the output instead of removing it.
        #[arg(long)]
        keep_delay: bool,
    },
}

fn read_wav(path: &PathBuf) -> Result<Vec<f32>> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    if spec.sample_rate != SAMPLE_RATE {
        bail!("{}: need {} Hz, got {} (resample with ffmpeg -ar 48000)", path.display(), SAMPLE_RATE, spec.sample_rate);
    }
    let ch = spec.channels as usize;
    let all: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    Ok(all.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Presets => {
            for p in Preset::ALL {
                let c = Config::from_preset(p);
                let ladder: Vec<_> = c.ladder.iter().map(|m| m.name()).collect();
                println!("{:<22} ladder: {}", p.name(), ladder.join(" > "));
                println!(
                    "{:<22} gate: opens within {} dB of your voice, ratio {}:1, up to -{} dB, hold {} ms, release {} ms\n",
                    "", c.gate.threshold_below_speech_db, c.gate.ratio, c.gate.max_atten_db, c.gate.hold_ms, c.gate.release_ms
                );
            }
        }
        Cmd::Calibrate { preset, seconds } => {
            let c = Config::from_preset(preset);
            let t = calibrate(&cli.models_dir, &c.ladder, seconds)?;
            println!("{:<8} {:>8} {:>10} {:>9} {:>9} {:>9}", "model", "hop ms", "delay ms", "mean ms", "p99 ms", "mean %hop");
            for x in &t {
                println!("{:<8} {:>8.1} {:>10.1} {:>9.2} {:>9.2} {:>8.0}%", x.model.name(), x.hop_ms, x.algorithmic_ms, x.mean_ms, x.p99_ms, 100.0 * x.mean_frac());
            }
            println!("\npick for '{}': {} (p99 <= 40% of hop, mean <= 25%)", preset.name(), pick(&c.ladder, &t, 0.4, 0.25).name());
        }
        Cmd::Process { preset, input, output, model, block, no_gate, gate_threshold, gate_ratio, gate_max, gate_hold, keep_delay } => {
            let mut cfg = Config::from_preset(preset);
            cfg.block = block;
            cfg.auto_govern = false;
            cfg.gate.enabled &= !no_gate;
            if let Some(t) = gate_threshold {
                cfg.gate.threshold_below_speech_db = t;
            }
            if let Some(v) = gate_ratio {
                cfg.gate.ratio = v;
            }
            if let Some(v) = gate_max {
                cfg.gate.max_atten_db = v;
            }
            if let Some(v) = gate_hold {
                cfg.gate.hold_ms = v;
            }
            let mut engine = Engine::new(cfg, &cli.models_dir, model)?;
            let delay = engine.total_delay();
            let samples = read_wav(&input)?;
            let mut padded = samples.clone();
            padded.resize((samples.len() + delay).div_ceil(block) * block, 0.0);
            let mut out = Vec::with_capacity(padded.len());
            let mut buf = vec![0.0f32; block];
            for chunk in padded.chunks(block) {
                engine.process(chunk, &mut buf[..chunk.len()])?;
                out.extend_from_slice(&buf[..chunk.len()]);
            }
            let out: Vec<f32> = if keep_delay { out } else { out.into_iter().skip(delay).take(samples.len()).collect() };
            let spec = hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::create(&output, spec)?;
            for s in &out {
                w.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
            }
            w.finalize()?;
            eprintln!(
                "preset {} | model {} | total delay {:.1} ms | gate {} | speech level learned {:.1} dBFS",
                preset.name(), engine.model().name(), engine.total_delay_ms(),
                if no_gate { "off" } else { "on" }, engine.gate().speech_reference_db()
            );
        }
    }
    Ok(())
}
