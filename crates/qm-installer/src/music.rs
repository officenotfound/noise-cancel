//! An original chiptune loop, synthesised at start-up: two pulse channels, a triangle bass and a noise drum track,
//! like the four-channel tracker tunes of 90s scene releases. A minor, 150 BPM, 8 bars.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const RATE: f32 = 44_100.0;
const BPM: f32 = 150.0;

fn hz(note: i32) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

fn pulse(phase: f32, duty: f32) -> f32 {
    if phase.fract() < duty { 1.0 } else { -1.0 }
}

fn triangle(phase: f32) -> f32 {
    4.0 * (phase.fract() - 0.5).abs() - 1.0
}

struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Render the whole loop as mono samples.
pub fn render() -> Vec<f32> {
    let step = (RATE * 60.0 / BPM / 4.0) as usize; // one 16th note
    // root, third, fifth for Am F C G
    let chords: [(i32, [i32; 3]); 4] = [(57, [0, 3, 7]), (53, [0, 4, 7]), (60, [0, 4, 7]), (55, [0, 4, 7])];
    let arp = [0usize, 1, 2, 1, 0, 1, 2, 1, 0, 1, 2, 1, 2, 1, 2, 1];
    // lead melody (semitones above A4 = 69) for bars 4..8; None = rest
    let mel: [[Option<i32>; 16]; 4] = [
        [Some(7), None, Some(3), None, Some(5), Some(7), None, Some(3), Some(0), None, None, None, Some(3), None, Some(5), None],
        [Some(5), None, Some(0), None, Some(3), Some(5), None, Some(0), Some(-2), None, None, None, Some(0), None, Some(3), None],
        [Some(7), None, Some(12), None, Some(10), Some(7), None, Some(3), Some(7), None, None, None, Some(10), None, Some(7), None],
        [Some(10), None, Some(7), None, Some(5), Some(7), None, Some(10), Some(14), None, Some(12), None, Some(10), None, Some(7), None],
    ];
    let bars = 8;
    let total = step * 16 * bars;
    let mut out = vec![0.0f32; total];
    let mut noise = Noise(0x1234_5678);
    let (mut p_arp, mut p_mel, mut p_bass) = (0.0f32, 0.0f32, 0.0f32);
    for bar in 0..bars {
        let (root, tones) = chords[bar % 4];
        for s in 0..16 {
            let t0 = (bar * 16 + s) * step;
            for i in 0..step {
                let n = t0 + i;
                let local = i as f32 / step as f32;
                // arpeggio: pulse 25%, fast decay, an octave up on the second half of each bar
                let note = root + 12 + tones[arp[s]] + if s >= 12 { 12 } else { 0 };
                p_arp += hz(note) / RATE;
                let env_a = (1.0 - local).powf(1.5);
                let mut v = 0.13 * pulse(p_arp, 0.25) * env_a;
                // melody in the second half
                if bar >= 4 {
                    if let Some(m) = mel[bar - 4][s] {
                        p_mel += hz(69 + m) / RATE;
                        let vib = 1.0 + 0.004 * ((n as f32 / RATE) * 40.0).sin();
                        v += 0.12 * pulse(p_mel * vib, 0.5) * (1.0 - local * 0.5);
                    }
                }
                // bass: triangle on 8ths, octave jump on the off-beat
                let bn = root - 24 + if (s / 2) % 2 == 1 { 12 } else { 0 };
                p_bass += hz(bn) / RATE;
                v += 0.28 * triangle(p_bass) * if s % 2 == 0 { 1.0 - local * 0.4 } else { 0.0 };
                // drums
                let secs = i as f32 / RATE;
                if s % 8 == 0 {
                    // kick: falling sine
                    v += 0.35 * (2.0 * std::f32::consts::PI * (110.0 * (-secs * 28.0).exp() + 40.0) * secs).sin() * (-secs * 12.0).exp();
                }
                if s % 8 == 4 {
                    v += 0.22 * noise.next() * (-secs * 18.0).exp();
                }
                if s % 2 == 0 && s % 8 != 4 {
                    v += 0.06 * noise.next() * (-secs * 90.0).exp();
                }
                out[n] = v;
            }
        }
    }
    // soft clip
    out.iter_mut().for_each(|v| *v = (*v * 1.2).tanh() * 0.8);
    out
}

/// Plays the loop on the default output device until dropped. Silent (and harmless) when there is no audio device.
pub struct Player {
    _stream: Option<cpal::Stream>,
    pub muted: Arc<AtomicBool>,
}

impl Player {
    pub fn silent() -> Self {
        Self { _stream: None, muted: Arc::new(AtomicBool::new(true)) }
    }

    pub fn start() -> Self {
        let muted = Arc::new(AtomicBool::new(false));
        Self { _stream: Self::open(muted.clone()).ok(), muted }
    }

    fn open(muted: Arc<AtomicBool>) -> anyhow::Result<cpal::Stream> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let dev = cpal::default_host().default_output_device().ok_or_else(|| anyhow::anyhow!("no output device"))?;
        let cfg = dev.default_output_config()?;
        let (ch, rate) = (cfg.channels() as usize, cfg.sample_rate() as f32);
        let samples = render();
        let (mut pos, step, mut fade) = (0.0f32, RATE / rate, 0.0f32);
        let mut cfg: cpal::StreamConfig = cfg.into();
        cfg.buffer_size = cpal::BufferSize::Default;
        let stream = dev.build_output_stream(
            cfg,
            move |data: &mut [f32], _| {
                let target = if muted.load(Ordering::Relaxed) { 0.0 } else { 1.0 };
                for frame in data.chunks_mut(ch.max(1)) {
                    fade += (target - fade) * 0.001;
                    let i = pos as usize % samples.len();
                    let v = samples[i] * 0.6 * fade;
                    frame.iter_mut().for_each(|o| *o = v);
                    pos += step;
                    if pos as usize >= samples.len() {
                        pos -= samples.len() as f32;
                    }
                }
            },
            |_| {},
            None,
        )?;
        stream.play()?;
        Ok(stream)
    }
}
