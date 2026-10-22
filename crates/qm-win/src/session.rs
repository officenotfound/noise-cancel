//! WASAPI shared-mode, event-driven capture -> processor -> render, on two real-time threads joined by a lock-free
//! ring buffer. The render side reads through [`DriftResampler`] to absorb clock drift between the devices.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use wasapi::{
    initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
};
use windows::core::w;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;

use crate::drift::{ratio_for_fill, DriftResampler, Source};

pub const SAMPLE_RATE: usize = 48_000;
/// Default ring fill the render side tries to hold (samples). One capture period; this is latency we add.
pub const DEFAULT_TARGET_FILL: usize = 480;
const RING_CAPACITY: usize = SAMPLE_RATE / 4;

/// What the capture thread hands each block to. Created *inside* the capture thread (it need not be `Send`).
pub trait Processor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()>;
}

pub type ProcessorFactory = Box<dyn FnOnce() -> Result<Box<dyn Processor>> + Send>;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

fn list(direction: Direction) -> Result<Vec<DeviceInfo>> {
    let _ = initialize_mta();
    let en = DeviceEnumerator::new()?;
    let mut out = Vec::new();
    for d in &en.get_device_collection(&direction)? {
        let d = d?;
        out.push(DeviceInfo { id: d.get_id()?, name: d.get_friendlyname()? });
    }
    Ok(out)
}

pub fn list_inputs() -> Result<Vec<DeviceInfo>> {
    list(Direction::Capture)
}

pub fn list_outputs() -> Result<Vec<DeviceInfo>> {
    list(Direction::Render)
}

fn default_device(direction: Direction) -> Result<DeviceInfo> {
    let _ = initialize_mta();
    let d = DeviceEnumerator::new()?.get_default_device(&direction)?;
    Ok(DeviceInfo { id: d.get_id()?, name: d.get_friendlyname()? })
}

pub fn default_input() -> Result<DeviceInfo> {
    default_device(Direction::Capture)
}

pub fn default_output() -> Result<DeviceInfo> {
    default_device(Direction::Render)
}

/// The VB-Audio Virtual Cable render endpoint ("CABLE Input"), if installed.
pub fn find_vb_cable() -> Result<Option<DeviceInfo>> {
    Ok(list_outputs()?.into_iter().find(|d| d.name.to_lowercase().contains("cable input")))
}

/// Raise the calling thread to the "Pro Audio" MMCSS class (best effort).
pub(crate) fn pro_audio_priority() {
    let mut idx = 0u32;
    unsafe {
        let _ = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut idx);
    }
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Capture device id; `None` = system default microphone.
    pub input_id: Option<String>,
    /// Render device id; `None` = system default speakers (used for testing without a virtual cable).
    pub output_id: Option<String>,
    pub target_fill: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self { input_id: None, output_id: None, target_fill: DEFAULT_TARGET_FILL }
    }
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    captured_frames: AtomicU64,
    rendered_frames: AtomicU64,
    underruns: AtomicU64,
    overflows: AtomicU64,
    ring_fill: AtomicUsize,
    capture_period_frames: AtomicUsize,
    /// Peak of the last block, as f32 bits.
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    error: Mutex<Option<String>>,
    input_name: Mutex<String>,
    output_name: Mutex<String>,
}

impl Shared {
    fn fail(&self, who: &str, e: impl std::fmt::Display) {
        let mut slot = self.error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(format!("{who}: {e}"));
        }
        self.stop.store(true, Ordering::SeqCst);
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
}

pub struct Session {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Session {
    /// Start capture and render threads. Returns immediately; poll [`Session::stats`] for errors.
    pub fn start(cfg: SessionConfig, factory: ProcessorFactory) -> Result<Session> {
        let shared = Arc::new(Shared::default());
        let (prod, cons) = rtrb::RingBuffer::<f32>::new(RING_CAPACITY);

        let cap = {
            let (sh, cfg) = (shared.clone(), cfg.clone());
            thread::Builder::new().name("qm-capture".into()).spawn(move || {
                if let Err(e) = capture_main(&sh, &cfg, factory, prod) {
                    sh.fail("capture", e);
                }
            })?
        };
        let ren = {
            let (sh, cfg) = (shared.clone(), cfg);
            thread::Builder::new().name("qm-render".into()).spawn(move || {
                if let Err(e) = render_main(&sh, &cfg, cons) {
                    sh.fail("render", e);
                }
            })?
        };
        Ok(Session { shared, threads: vec![cap, ren] })
    }

    pub fn stats(&self) -> Stats {
        let s = &self.shared;
        Stats {
            captured_frames: s.captured_frames.load(Ordering::Relaxed),
            rendered_frames: s.rendered_frames.load(Ordering::Relaxed),
            underruns: s.underruns.load(Ordering::Relaxed),
            overflows: s.overflows.load(Ordering::Relaxed),
            ring_fill: s.ring_fill.load(Ordering::Relaxed),
            capture_period_frames: s.capture_period_frames.load(Ordering::Relaxed),
            input_peak: f32::from_bits(s.input_peak.load(Ordering::Relaxed)),
            output_peak: f32::from_bits(s.output_peak.load(Ordering::Relaxed)),
            input_name: s.input_name.lock().unwrap().clone(),
            output_name: s.output_name.lock().unwrap().clone(),
            error: s.error.lock().unwrap().clone(),
        }
    }

    /// True once a thread has stopped because of an error (device unplugged, format change, ...).
    pub fn failed(&self) -> bool {
        self.shared.error.lock().unwrap().is_some()
    }

    pub fn stop(mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn mono_f32_format() -> WaveFormat {
    WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE, 1, None)
}

fn capture_main(sh: &Shared, cfg: &SessionConfig, factory: ProcessorFactory, mut prod: rtrb::Producer<f32>) -> Result<()> {
    let _ = initialize_mta();
    pro_audio_priority();
    let en = DeviceEnumerator::new()?;
    let dev = match &cfg.input_id {
        Some(id) => en.get_device(id)?,
        None => en.get_default_device(&Direction::Capture)?,
    };
    *sh.input_name.lock().unwrap() = dev.get_friendlyname().unwrap_or_default();
    let mut client = dev.get_iaudioclient()?;
    let (default_period, _min) = client.get_device_period()?;
    client.initialize_client(
        &mono_f32_format(),
        &Direction::Capture,
        &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: default_period },
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    sh.capture_period_frames.store((default_period as usize * SAMPLE_RATE) / 10_000_000, Ordering::Relaxed);

    // Build the processor here, on this thread; model loading can take a few hundred ms.
    let mut processor = factory()?;
    client.start_stream()?;

    let mut bytes: VecDeque<u8> = VecDeque::with_capacity(SAMPLE_RATE);
    let mut input: Vec<f32> = Vec::with_capacity(4096);
    let mut output: Vec<f32> = Vec::with_capacity(4096);
    while !sh.stop.load(Ordering::Relaxed) {
        if event.wait_for_event(1000).is_err() {
            // No data for a second: treat as a dead device so the supervisor restarts the session.
            return Err(anyhow!("no audio from the microphone for 1 s"));
        }
        // Drain every packet that is ready.
        while let Some(n) = capture.get_next_packet_size()? {
            if n == 0 {
                break;
            }
            capture.read_from_device_to_deque(&mut bytes)?;
        }
        let samples = bytes.len() / 4;
        if samples == 0 {
            continue;
        }
        input.clear();
        for _ in 0..samples {
            let b = [bytes.pop_front().unwrap(), bytes.pop_front().unwrap(), bytes.pop_front().unwrap(), bytes.pop_front().unwrap()];
            input.push(f32::from_le_bytes(b));
        }
        output.clear();
        output.resize(samples, 0.0);
        processor.process(&input, &mut output)?;
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        sh.input_peak.store(peak(&input).to_bits(), Ordering::Relaxed);
        sh.output_peak.store(peak(&output).to_bits(), Ordering::Relaxed);
        sh.captured_frames.fetch_add(samples as u64, Ordering::Relaxed);
        for &s in &output {
            if prod.push(s).is_err() {
                sh.overflows.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }
    client.stop_stream()?;
    Ok(())
}

struct RingSource(rtrb::Consumer<f32>);

impl Source for RingSource {
    fn pop(&mut self) -> Option<f32> {
        self.0.pop().ok()
    }
    fn available(&self) -> usize {
        self.0.slots()
    }
}

fn render_main(sh: &Shared, cfg: &SessionConfig, cons: rtrb::Consumer<f32>) -> Result<()> {
    let _ = initialize_mta();
    pro_audio_priority();
    let en = DeviceEnumerator::new()?;
    let dev = match &cfg.output_id {
        Some(id) => en.get_device(id)?,
        None => en.get_default_device(&Direction::Render)?,
    };
    *sh.output_name.lock().unwrap() = dev.get_friendlyname().unwrap_or_default();
    let mut client = dev.get_iaudioclient()?;
    let (default_period, _min) = client.get_device_period()?;
    client.initialize_client(
        &mono_f32_format(),
        &Direction::Render,
        &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: default_period },
    )?;
    let event = client.set_get_eventhandle()?;
    let render = client.get_audiorenderclient()?;

    // Start with a buffer of silence so playback begins cleanly.
    let space = client.get_available_space_in_frames()? as usize;
    render.write_to_device(space, &vec![0u8; space * 4], None)?;
    client.start_stream()?;

    let mut src = RingSource(cons);
    let mut resampler = DriftResampler::new();
    let mut waiting_to_prime = true;
    let mut out_f32: Vec<f32> = Vec::new();
    let mut out_bytes: Vec<u8> = Vec::new();
    while !sh.stop.load(Ordering::Relaxed) {
        if event.wait_for_event(1000).is_err() {
            return Err(anyhow!("virtual cable stopped asking for audio for 1 s"));
        }
        let frames = client.get_available_space_in_frames()? as usize;
        if frames == 0 {
            continue;
        }
        let fill = src.available();
        sh.ring_fill.store(fill, Ordering::Relaxed);
        out_f32.clear();
        out_f32.resize(frames, 0.0);

        if waiting_to_prime {
            // After start or an underrun, hold silence until the ring has refilled to target (avoids repeated crackle).
            if fill >= cfg.target_fill {
                waiting_to_prime = false;
                resampler.reset();
            }
        }
        if !waiting_to_prime {
            let real = resampler.process(&mut out_f32, &mut src, ratio_for_fill(fill, cfg.target_fill));
            if real < frames {
                sh.underruns.fetch_add(1, Ordering::Relaxed);
                waiting_to_prime = true;
            }
        }

        out_bytes.clear();
        out_bytes.extend(out_f32.iter().flat_map(|s| s.to_le_bytes()));
        render.write_to_device(frames, &out_bytes, None)?;
        sh.rendered_frames.fetch_add(frames as u64, Ordering::Relaxed);
    }
    thread::sleep(Duration::from_millis(20));
    client.stop_stream()?;
    Ok(())
}
