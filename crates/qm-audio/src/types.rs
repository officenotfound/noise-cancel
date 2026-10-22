//! Types shared by the Windows (WASAPI) and macOS (CoreAudio) audio backends.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::Result;

pub const SAMPLE_RATE: usize = 48_000;
/// Default ring fill the render side tries to hold (samples). One capture period; this is latency we add.
pub const DEFAULT_TARGET_FILL: usize = 480;
/// Default starting output cushion, in device callbacks. It grows by itself (up to `MAX_CUSHION`) after a dropout.
#[cfg(target_os = "macos")]
pub const DEFAULT_CUSHION: f32 = 2.0;
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_CUSHION: f32 = 1.5;
pub const MAX_CUSHION: f32 = 3.0;
pub(crate) const RING_CAPACITY: usize = SAMPLE_RATE / 4;

/// What the capture side hands each block to. Created *inside* the audio thread (it need not be `Send`).
pub trait Processor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()>;
}

pub type ProcessorFactory = Box<dyn FnOnce() -> Result<Box<dyn Processor>> + Send>;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Capture device id; `None` = system default microphone.
    pub input_id: Option<String>,
    /// Render device id; `None` = system default speakers (used for testing without a virtual cable).
    pub output_id: Option<String>,
    pub target_fill: usize,
    /// Capture and process but send the result nowhere (test mode when no virtual cable is installed).
    pub discard_output: bool,
    /// Output ring cushion in device callbacks (the ring is held at this many callbacks of audio).
    pub cushion: f32,
    /// macOS: run the processor inside CoreAudio's real-time input callback instead of a separate thread (default on
    /// macOS: measured 0 stalls over 5 ms per 6 s versus 25-31 with a separate thread, because the OS treats the
    /// callback thread as real-time and keeps the CPU clocked up).
    pub inline_dsp: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self { input_id: None, output_id: None, target_fill: DEFAULT_TARGET_FILL, discard_output: false, cushion: DEFAULT_CUSHION, inline_dsp: cfg!(target_os = "macos") }
    }
}

#[derive(Clone, Debug)]
pub struct Stats {
    pub captured_frames: u64,
    pub rendered_frames: u64,
    pub underruns: u64,
    pub overflows: u64,
    pub ring_fill: usize,
    pub capture_period_frames: usize,
    /// Peak level (linear, 0..1) of the most recent block before and after processing.
    pub input_peak: f32,
    pub output_peak: f32,
    pub input_name: String,
    pub output_name: String,
    pub error: Option<String>,
    /// Diagnostics (macOS): lowest ring fill seen at the start of an output callback, and the slowest processing block.
    pub min_fill: Option<usize>,
    /// Slowest processing block after the first 100 (start-up blocks are slower and not counted).
    pub max_dsp_us: u64,
    /// The output cushion in use now, in device callbacks (grows by itself after a dropout).
    pub cushion: f32,
    /// Number of processing blocks slower than 5 ms after start-up.
    pub slow_blocks: u64,
}

/// State shared between a session's threads and its owner.
#[derive(Default)]
pub(crate) struct Shared {
    pub stop: AtomicBool,
    pub captured_frames: AtomicU64,
    pub rendered_frames: AtomicU64,
    pub underruns: AtomicU64,
    pub overflows: AtomicU64,
    pub ring_fill: AtomicUsize,
    pub capture_period_frames: AtomicUsize,
    /// Peak of the last block, as f32 bits.
    pub input_peak: AtomicU32,
    pub output_peak: AtomicU32,
    /// Lowest ring fill + 1 (0 = not measured yet) and slowest processing block in microseconds.
    pub min_fill_plus1: AtomicUsize,
    pub max_dsp_us: AtomicU64,
    pub cushion_bits: AtomicU32,
    pub slow_blocks: AtomicU64,
    pub error: Mutex<Option<String>>,
    pub input_name: Mutex<String>,
    pub output_name: Mutex<String>,
}

impl Shared {
    pub fn fail(&self, who: &str, e: impl std::fmt::Display) {
        let mut slot = self.error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(format!("{who}: {e}"));
        }
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn stats(&self) -> Stats {
        Stats {
            captured_frames: self.captured_frames.load(Ordering::Relaxed),
            rendered_frames: self.rendered_frames.load(Ordering::Relaxed),
            underruns: self.underruns.load(Ordering::Relaxed),
            overflows: self.overflows.load(Ordering::Relaxed),
            ring_fill: self.ring_fill.load(Ordering::Relaxed),
            capture_period_frames: self.capture_period_frames.load(Ordering::Relaxed),
            input_peak: f32::from_bits(self.input_peak.load(Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak.load(Ordering::Relaxed)),
            input_name: self.input_name.lock().unwrap().clone(),
            output_name: self.output_name.lock().unwrap().clone(),
            error: self.error.lock().unwrap().clone(),
            min_fill: self.min_fill_plus1.load(Ordering::Relaxed).checked_sub(1),
            max_dsp_us: self.max_dsp_us.load(Ordering::Relaxed),
            cushion: f32::from_bits(self.cushion_bits.load(Ordering::Relaxed)),
            slow_blocks: self.slow_blocks.load(Ordering::Relaxed),
        }
    }

    pub fn store_peaks(&self, input: &[f32], output: &[f32]) {
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        self.input_peak.store(peak(input).to_bits(), Ordering::Relaxed);
        self.output_peak.store(peak(output).to_bits(), Ordering::Relaxed);
    }
}
