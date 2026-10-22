use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use qm_engine::Engine;
use qm_win::Processor;

use crate::recorder::Recorder;

/// Live engine numbers the supervisor shows in the tray.
#[derive(Clone, Debug, Default)]
pub struct EngineStatus {
    pub model: &'static str,
    pub delay_ms: f64,
    pub load: f64,
}

/// Runs the engine on the capture thread; `bypass` passes the microphone through untouched.
pub struct EngineProcessor {
    engine: Engine,
    bypass: Arc<AtomicBool>,
    status: Arc<Mutex<EngineStatus>>,
    recorder: Arc<Recorder>,
    calls: u32,
}

impl EngineProcessor {
    pub fn new(engine: Engine, bypass: Arc<AtomicBool>, status: Arc<Mutex<EngineStatus>>, recorder: Arc<Recorder>) -> Self {
        let mut p = Self { engine, bypass, status, recorder, calls: 0 };
        p.publish();
        p
    }

    fn publish(&mut self) {
        // try_lock: never block the audio thread on the UI.
        if let Ok(mut st) = self.status.try_lock() {
            *st = EngineStatus {
                model: self.engine.model().name(),
                delay_ms: self.engine.total_delay_ms(),
                load: self.engine.load(),
            };
        }
    }
}

impl Processor for EngineProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        if self.bypass.load(Ordering::Relaxed) {
            output.copy_from_slice(input);
        } else {
            self.engine.process(input, output)?;
        }
        self.recorder.push(input, output);
        self.calls = self.calls.wrapping_add(1);
        if self.calls % 25 == 0 {
            self.publish();
        }
        Ok(())
    }
}
