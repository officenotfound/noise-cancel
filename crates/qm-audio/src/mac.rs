//! macOS audio through CoreAudio (via `cpal`): microphone -> processor -> virtual cable (BlackHole), and playback for
//! the in-app recorder.
//!
//! Layout: CoreAudio's own real-time callbacks only move samples. The input callback converts to mono and pushes
//! into a ring; a DSP thread resamples to 48 kHz when the device runs at another rate, runs the processor, and
//! pushes 48 kHz audio into a second ring; the output callback pulls from that ring through the drift-compensating
//! resampler (which also converts to the output device's rate) and writes the same signal to every channel.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::drift::{ratio_for_fill, DriftResampler, Source};
use crate::player_core::PlayShared;
use crate::types::*;

fn info(d: &cpal::Device) -> Option<DeviceInfo> {
    Some(DeviceInfo { id: d.id().ok()?.to_string(), name: d.description().ok()?.name().to_string() })
}

fn find(id: &Option<String>, input: bool) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match id {
        Some(id) => {
            let did = id.parse().map_err(|e| anyhow!("bad device id {id:?}: {e}"))?;
            host.device_by_id(&did).ok_or_else(|| anyhow!("audio device {id:?} not found"))
        }
        None => if input { host.default_input_device() } else { host.default_output_device() }
            .ok_or_else(|| anyhow!("no default {} device", if input { "input" } else { "output" })),
    }
}

pub fn list_inputs() -> Result<Vec<DeviceInfo>> {
    Ok(cpal::default_host().input_devices()?.filter_map(|d| info(&d)).collect())
}

pub fn list_outputs() -> Result<Vec<DeviceInfo>> {
    Ok(cpal::default_host().output_devices()?.filter_map(|d| info(&d)).collect())
}

pub fn default_input() -> Result<DeviceInfo> {
    cpal::default_host().default_input_device().and_then(|d| info(&d)).ok_or_else(|| anyhow!("no default input device"))
}

pub fn default_output() -> Result<DeviceInfo> {
    cpal::default_host().default_output_device().and_then(|d| info(&d)).ok_or_else(|| anyhow!("no default output device"))
}

/// The BlackHole virtual audio device (the macOS counterpart of VB-Cable), if installed. Prefers the 2-channel one.
pub fn find_virtual_cable() -> Result<Option<DeviceInfo>> {
    let outs: Vec<DeviceInfo> = list_outputs()?.into_iter().filter(|d| d.name.to_lowercase().contains("blackhole")).collect();
    Ok(outs.iter().find(|d| d.name.contains("2ch")).cloned().or_else(|| outs.into_iter().next()))
}

/// Ask macOS to schedule this thread like an interactive audio thread (best effort).
fn realtime_priority() {
    unsafe {
        let _ = libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
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

pub struct Session {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Session {
    /// Start capture, processing and render. Returns immediately; poll [`Session::stats`] for errors.
    pub fn start(cfg: SessionConfig, factory: ProcessorFactory) -> Result<Session> {
        let shared = Arc::new(Shared::default());
        let sh = shared.clone();
        let control = thread::Builder::new().name("qm-audio-control".into()).spawn(move || {
            if let Err(e) = run_session(&sh, &cfg, factory) {
                sh.fail("audio", format!("{e:#}"));
            }
        })?;
        Ok(Session { shared, threads: vec![control] })
    }

    pub fn stats(&self) -> Stats {
        self.shared.stats()
    }

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

/// Owns the CoreAudio streams (they must stay on this thread) and supervises the DSP thread.
fn run_session(sh: &Arc<Shared>, cfg: &SessionConfig, factory: ProcessorFactory) -> Result<()> {
    let in_dev = find(&cfg.input_id, true)?;
    let out_dev = if cfg.discard_output { None } else { Some(find(&cfg.output_id, false)?) };
    *sh.input_name.lock().unwrap() = in_dev.description().map(|d| d.name().to_string()).unwrap_or_default();
    *sh.output_name.lock().unwrap() = out_dev.as_ref().and_then(|d| d.description().ok()).map(|d| d.name().to_string()).unwrap_or_default();

    let in_cfg = in_dev.default_input_config()?;
    let out_cfg = out_dev.as_ref().map(|d| d.default_output_config()).transpose()?;
    let (in_ch, in_rate) = (in_cfg.channels() as usize, in_cfg.sample_rate());
    if in_cfg.sample_format() != cpal::SampleFormat::F32 || out_cfg.as_ref().map_or(false, |c| c.sample_format() != cpal::SampleFormat::F32) {
        return Err(anyhow!("unsupported audio format (need 32-bit float)"));
    }

    let (mut in_prod, in_cons) = rtrb::RingBuffer::<f32>::new(in_rate as usize);
    let (out_prod, out_cons) = rtrb::RingBuffer::<f32>::new(RING_CAPACITY);

    // The processor (model loading takes a few hundred ms) is built here, before any audio flows.
    let processor = factory()?;
    let dsp = Dsp::new(processor, in_rate as f64);
    let discard = cfg.discard_output;

    // ---- input stream, and the DSP either inline (in the real-time callback) or on its own thread
    let in_stream_cfg = cpal::StreamConfig { channels: in_cfg.channels(), sample_rate: in_rate, buffer_size: cpal::BufferSize::Default };
    let sh_in = sh.clone();
    let err_in = {
        let sh = sh.clone();
        move |e| sh.fail("microphone", e)
    };
    let mut dsp_thread = None;
    let in_stream = if cfg.inline_dsp {
        let mut dsp = SendBox(dsp);
        let mut out_prod = SendBox(out_prod);
        let mut mono: Vec<f32> = Vec::with_capacity(8192);
        let mut qos_set = false;
        in_dev.build_input_stream(
            in_stream_cfg,
            move |data: &[f32], _| {
                if !qos_set {
                    realtime_priority();
                    qos_set = true;
                }
                sh_in.capture_period_frames.store(data.len() / in_ch.max(1), Ordering::Relaxed);
                mono.clear();
                for frame in data.chunks_exact(in_ch.max(1)) {
                    mono.push(frame.iter().sum::<f32>() / in_ch.max(1) as f32);
                }
                if let Err(e) = dsp.get().feed(&mono, &sh_in, out_prod.get(), discard) {
                    sh_in.fail("processing", format!("{e:#}"));
                }
            },
            err_in,
            None,
        )?
    } else {
        let sh_dsp = sh.clone();
        let boxed = SendBox((dsp, in_cons, out_prod));
        dsp_thread = Some(thread::Builder::new().name("qm-dsp".into()).spawn(move || {
            realtime_priority();
            let (dsp, in_cons, out_prod) = boxed.into_inner();
            if let Err(e) = dsp_thread_main(&sh_dsp, in_cons, out_prod, dsp, discard) {
                sh_dsp.fail("processing", format!("{e:#}"));
            }
        })?);
        in_dev.build_input_stream(
            in_stream_cfg,
            move |data: &[f32], _| {
                sh_in.capture_period_frames.store(data.len() / in_ch.max(1), Ordering::Relaxed);
                for frame in data.chunks_exact(in_ch.max(1)) {
                    let mono = frame.iter().sum::<f32>() / in_ch.max(1) as f32;
                    if in_prod.push(mono).is_err() {
                        sh_in.overflows.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            err_in,
            None,
        )?
    };

    // ---- output stream (absent in test mode)
    let out_stream = match (&out_dev, &out_cfg) {
        (Some(out_dev), Some(out_cfg)) => {
            let (out_ch, out_rate) = (out_cfg.channels() as usize, out_cfg.sample_rate());
            let out_stream_cfg = cpal::StreamConfig { channels: out_cfg.channels(), sample_rate: out_rate, buffer_size: cpal::BufferSize::Default };
            let sh_out = sh.clone();
            let target_fill = cfg.target_fill;
            let mut cushion = cfg.cushion;
            sh.cushion_bits.store(cushion.to_bits(), Ordering::Relaxed);
            let base_ratio = SAMPLE_RATE as f64 / out_rate as f64; // 48 kHz samples consumed per output sample
            let mut src = RingSource(out_cons);
            let mut resampler = DriftResampler::new();
            let mut priming = true;
            let mut mono: Vec<f32> = Vec::new();
            Some(out_dev.build_output_stream(
                out_stream_cfg,
                move |data: &mut [f32], _| {
                    let frames = data.len() / out_ch.max(1);
                    let fill = src.available();
                    sh_out.ring_fill.store(fill, Ordering::Relaxed);
                    // The cushion must cover at least one and a half device callbacks (in 48 kHz samples), or the
                    // ring runs dry between processing bursts and the output crackles.
                    let target = target_fill.max((frames as f64 * base_ratio * cushion as f64) as usize);
                    if !priming {
                        let seen = sh_out.min_fill_plus1.load(Ordering::Relaxed);
                        if seen == 0 || fill + 1 < seen {
                            sh_out.min_fill_plus1.store(fill + 1, Ordering::Relaxed);
                        }
                    }
                    mono.clear();
                    mono.resize(frames, 0.0);
                    if priming && fill >= target {
                        priming = false;
                        resampler.reset();
                    }
                    if !priming {
                        let real = resampler.process(&mut mono, &mut src, base_ratio * ratio_for_fill(fill, target));
                        if real < frames {
                            sh_out.underruns.fetch_add(1, Ordering::Relaxed);
                            priming = true;
                            // A dropout means the cushion was too small for this machine's scheduling jitter: grow it.
                            cushion = (cushion + 0.5).min(MAX_CUSHION);
                            sh_out.cushion_bits.store(cushion.to_bits(), Ordering::Relaxed);
                        }
                    }
                    for (frame, m) in data.chunks_exact_mut(out_ch.max(1)).zip(&mono) {
                        frame.fill(*m);
                    }
                    sh_out.rendered_frames.fetch_add(frames as u64, Ordering::Relaxed);
                },
                {
                    let sh = sh.clone();
                    move |e| sh.fail("output device", e)
                },
                None,
            )?)
        }
        _ => None,
    };

    in_stream.play()?;
    if let Some(o) = &out_stream {
        o.play()?;
    }
    while !sh.stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
    drop(in_stream);
    drop(out_stream);
    if let Some(t) = dsp_thread {
        let _ = t.join();
    }
    Ok(())
}

/// Wrapper to move a value to the one thread that will use it. The processor is built on the control thread and then
/// used by exactly one audio thread; nothing in it is tied to the thread that created it.
struct SendBox<T>(T);
unsafe impl<T> Send for SendBox<T> {}

impl<T> SendBox<T> {
    // Going through a method makes closures capture the whole wrapper (not just the field inside it).
    fn get(&mut self) -> &mut T {
        &mut self.0
    }

    fn into_inner(self) -> T {
        self.0
    }
}

/// Native-rate mono in, processed 48 kHz out: sample-rate conversion, the processor, stats, and the output ring.
struct Dsp {
    processor: Box<dyn Processor>,
    ratio: f64,
    convert: bool,
    queue: VecDeque<f32>,
    resampler: DriftResampler,
    block: Vec<f32>,
    output: Vec<f32>,
    blocks: u64,
}

impl Dsp {
    fn new(processor: Box<dyn Processor>, in_rate: f64) -> Self {
        let ratio = in_rate / SAMPLE_RATE as f64; // native samples consumed per 48 kHz sample
        Self {
            processor,
            ratio,
            convert: (ratio - 1.0).abs() > 1e-6,
            queue: VecDeque::new(),
            resampler: DriftResampler::new(),
            block: Vec::with_capacity(8192),
            output: Vec::with_capacity(8192),
            blocks: 0,
        }
    }

    fn feed(&mut self, native: &[f32], sh: &Shared, out_prod: &mut rtrb::Producer<f32>, discard: bool) -> Result<()> {
        self.block.clear();
        if self.convert {
            self.queue.extend(native.iter().copied());
            let out_n = ((self.queue.len() as f64 - 2.0) / self.ratio).floor().max(0.0) as usize;
            if out_n == 0 {
                return Ok(());
            }
            self.block.resize(out_n, 0.0);
            self.resampler.process(&mut self.block, &mut self.queue, self.ratio);
        } else {
            self.block.extend_from_slice(native);
        }
        if self.block.is_empty() {
            return Ok(());
        }
        self.output.clear();
        self.output.resize(self.block.len(), 0.0);
        let t0 = std::time::Instant::now();
        self.processor.process(&self.block, &mut self.output)?;
        self.blocks += 1;
        if self.blocks > 100 {
            let us = t0.elapsed().as_micros() as u64;
            sh.max_dsp_us.fetch_max(us, Ordering::Relaxed);
            if us > 5_000 {
                sh.slow_blocks.fetch_add(1, Ordering::Relaxed);
            }
        }
        sh.store_peaks(&self.block, &self.output);
        sh.captured_frames.fetch_add(self.block.len() as u64, Ordering::Relaxed);
        if !discard {
            for &s in &self.output {
                if out_prod.push(s).is_err() {
                    sh.overflows.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Separate-thread mode: pull native-rate mono from the input ring and feed the processor.
fn dsp_thread_main(sh: &Shared, mut in_cons: rtrb::Consumer<f32>, mut out_prod: rtrb::Producer<f32>, mut dsp: Dsp, discard: bool) -> Result<()> {
    let mut native: Vec<f32> = Vec::with_capacity(8192);
    while !sh.stop.load(Ordering::Relaxed) {
        let n = in_cons.slots().min(8192);
        if n == 0 {
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        native.clear();
        for _ in 0..n {
            if let Ok(s) = in_cons.pop() {
                native.push(s);
            }
        }
        dsp.feed(&native, sh, &mut out_prod, discard)?;
    }
    Ok(())
}

/// Plays a [`PlayShared`] buffer on an output device (default speakers/headphones), for the in-app recorder.
pub struct Player {
    stop: Arc<std::sync::atomic::AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    pub fn start(shared: Arc<PlayShared>, output_id: Option<String>) -> Result<Player> {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let (s2, e2) = (stop.clone(), error.clone());
        let handle = thread::Builder::new().name("qm-player".into()).spawn(move || {
            if let Err(e) = play_main(shared, &s2, output_id) {
                *e2.lock().unwrap() = Some(format!("{e:#}"));
            }
        })?;
        Ok(Player { stop, error, handle: Some(handle) })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn play_main(shared: Arc<PlayShared>, stop: &std::sync::atomic::AtomicBool, output_id: Option<String>) -> Result<()> {
    let dev = find(&output_id, false)?;
    let cfg = dev.default_output_config()?;
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return Err(anyhow!("unsupported output format {}", cfg.sample_format()));
    }
    let (ch, rate) = (cfg.channels() as usize, cfg.sample_rate());
    let ratio = SAMPLE_RATE as f64 / rate as f64;
    let stream_cfg = cpal::StreamConfig { channels: cfg.channels(), sample_rate: rate, buffer_size: cpal::BufferSize::Default };
    let mut queue: VecDeque<f32> = VecDeque::new();
    let mut resampler = DriftResampler::new();
    let mut tmp: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let stream = dev.build_output_stream(
        stream_cfg,
        move |data: &mut [f32], _| {
            let frames = data.len() / ch.max(1);
            let need = (frames as f64 * ratio).ceil() as usize + 2;
            if queue.len() < need {
                tmp.clear();
                tmp.resize(need - queue.len(), 0.0);
                shared.fill(&mut tmp);
                queue.extend(tmp.iter().copied());
            }
            mono.clear();
            mono.resize(frames, 0.0);
            resampler.process(&mut mono, &mut queue, ratio);
            for (frame, m) in data.chunks_exact_mut(ch.max(1)).zip(&mono) {
                frame.fill(*m);
            }
        },
        |_| {},
        None,
    )?;
    stream.play()?;
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(40));
    drop(stream);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_listing_works_without_microphone_access() {
        // Enumerating devices does not open them, so it needs no permission and must not panic.
        let _ = list_inputs();
        let _ = list_outputs();
        let _ = find_virtual_cable();
    }
}
