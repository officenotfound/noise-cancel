//! `QuietMic.exe --selftest SECONDS [--preset P] [--model M] [--input-id ID]`
//! Headless check of the whole audio path without a virtual cable: capture the microphone, run the engine,
//! record input and output to WAV files, and mute the render side (no feedback). Results go to `selftest.txt`.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use qm_engine::{Config, Engine, ModelId, Preset};
use qm_win::{Processor, Session, SessionConfig};

use super::{flag_value, logging::log, models_dir};

#[derive(Default)]
struct Recording {
    input: Vec<f32>,
    output: Vec<f32>,
}

struct RecordingProcessor {
    engine: Engine,
    rec: Arc<Mutex<Recording>>,
}

impl Processor for RecordingProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        self.engine.process(input, output)?;
        {
            let mut r = self.rec.lock().unwrap();
            r.input.extend_from_slice(input);
            r.output.extend_from_slice(output);
        }
        output.fill(0.0); // muted render
        Ok(())
    }
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
    }
    w.finalize()?;
    Ok(())
}

pub fn run(args: &[String], dir: &Path) {
    let secs: u64 = flag_value(args, "--selftest").and_then(|s| s.parse().ok()).unwrap_or(5);
    let preset: Preset = flag_value(args, "--preset").and_then(|s| s.parse().ok()).unwrap_or(Preset::LowLatency);
    let model: Option<ModelId> = flag_value(args, "--model").and_then(|s| s.parse().ok());
    let mut report = String::new();
    let result = (|| -> Result<()> {
        let mut cfg = Config::from_preset(preset);
        cfg.auto_govern = false;
        let rec = Arc::new(Mutex::new(Recording::default()));
        let rec2 = rec.clone();
        let models = models_dir();
        let factory: qm_win::ProcessorFactory = Box::new(move || {
            Ok(Box::new(RecordingProcessor { engine: Engine::new(cfg, models, model)?, rec: rec2 }) as Box<dyn Processor>)
        });
        let sc = SessionConfig { input_id: flag_value(args, "--input-id"), ..Default::default() };
        let session = Session::start(sc, factory)?;
        std::thread::sleep(Duration::from_secs(secs));
        let st = session.stats();
        session.stop();
        let r = rec.lock().unwrap();
        report.push_str(&format!(
            "selftest {secs}s preset={} model={}\ninput device : {}\noutput device: {}\ncaptured frames: {} ({:.1} s)\nrendered frames: {}\nunderruns: {}  overflows: {}\ncapture period: {} frames\nerror: {:?}\n",
            preset.name(), model.map(|m| m.name()).unwrap_or("preset default"), st.input_name, st.output_name,
            st.captured_frames, st.captured_frames as f64 / 48_000.0, st.rendered_frames, st.underruns, st.overflows,
            st.capture_period_frames, st.error
        ));
        if !r.input.is_empty() {
            let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
            report.push_str(&format!("input rms {:.5}, output rms {:.5}\n", rms(&r.input), rms(&r.output)));
            write_wav(&dir.join("selftest_input.wav"), &r.input)?;
            write_wav(&dir.join("selftest_output.wav"), &r.output)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        report.push_str(&format!("FAILED: {e:#}\n"));
    }
    log(&report);
    let _ = std::fs::write(dir.join("selftest.txt"), &report);
}
