//! Original chiptune loops, synthesised at start-up: pulse channels, a triangle bass and a noise drum track, like the
//! four-channel tracker tunes of 90s scene releases. `Mood::Happy` (installer): A minor, 150 BPM, 8 bars of arpeggios.
//! `Mood::Sad` (uninstaller): 76 BPM, a slow falling melody over Am F Dm E with a soft echo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const RATE: f32 = 44_100.0;
#[derive(Clone, Copy, PartialEq)]
pub enum Mood {
    Happy,
    Sad,
}

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
pub fn render(mood: Mood) -> Vec<f32> {
    if mood == Mood::Sad {
        return render_sad();
    }
    let step = (RATE * 60.0 / 150.0 / 4.0) as usize; // one 16th note
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

/// The uninstaller tune: slow, soft, minor. Notes are (start step, semitones above A4, length in steps) per bar of 16 steps.
fn render_sad() -> Vec<f32> {
    let step = (RATE * 60.0 / 76.0 / 4.0) as usize;
    // root and chord tones for Am F Dm E (E major leans back into A minor)
    let chords: [(i32, [i32; 3]); 4] = [(57, [0, 3, 7]), (53, [0, 4, 7]), (50, [0, 3, 7]), (52, [0, 4, 7])];
    let melody: [&[(usize, i32, usize)]; 4] = [
        &[(0, 7, 6), (6, 5, 2), (8, 3, 8)],
        &[(0, 0, 8), (8, -2, 8)],
        &[(0, 8, 6), (6, 7, 2), (8, 5, 8)],
        &[(0, 4, 8), (8, 3, 4), (12, 4, 4)],
    ];
    let bars = 8;
    let mut out = vec![0.0f32; step * 16 * bars];
    let mut noise = Noise(0x9e37_79b9);
    let (mut p_lead, mut p_arp, mut p_bass) = (0.0f32, 0.0f32, 0.0f32);
    for bar in 0..bars {
        let (root, tones) = chords[bar % 4];
        let lead_duty = if bar < 4 { 0.125 } else { 0.25 };
        let lead_gain = if bar < 4 { 0.11 } else { 0.14 };
        for s in 0..16 {
            let t0 = (bar * 16 + s) * step;
            let note = melody[bar % 4].iter().find(|(st, _, len)| s >= *st && s < st + len).copied();
            for i in 0..step {
                let n = t0 + i;
                let secs = i as f32 / RATE;
                let mut v = 0.0;
                // lead: slow pulse with a late vibrato and a gentle fade at the end of each note
                if let Some((st, off, len)) = note {
                    let into = ((s - st) * step + i) as f32 / (len * step) as f32;
                    let vib = 1.0 + 0.006 * (into * 5.0).max(0.0).min(1.0) * ((n as f32 / RATE) * 5.5).sin();
                    p_lead += hz(69 + off) * vib / RATE;
                    v += lead_gain * pulse(p_lead, lead_duty) * (1.0 - 0.45 * into);
                }
                // soft arpeggio on every other 16th, high and quiet
                if s % 2 == 0 {
                    p_arp += hz(root + 24 + tones[(s / 2) % 3]) / RATE;
                    v += 0.045 * pulse(p_arp, 0.25) * (1.0 - secs * RATE / step as f32).max(0.0).powf(1.2);
                }
                // bass: triangle on beats 1 and 3
                let held = s < 6 || (8..14).contains(&s);
                p_bass += hz(root - 24) / RATE;
                if held {
                    v += 0.26 * triangle(p_bass) * (1.0 - 0.3 * ((s % 8) as f32 / 6.0));
                }
                // a single soft kick per bar and a very quiet brushed hat on the off-beats
                if s == 0 {
                    v += 0.22 * (2.0 * std::f32::consts::PI * (80.0 * (-secs * 22.0).exp() + 38.0) * secs).sin() * (-secs * 9.0).exp();
                }
                if s == 4 || s == 12 {
                    v += 0.035 * noise.next() * (-secs * 40.0).exp();
                }
                out[n] = v;
            }
        }
    }
    // echo, for the empty-room feeling (fed back so the end of the loop rings into the start)
    let delay = (RATE * 0.395) as usize;
    let len = out.len();
    for pass in 0..2 {
        for n in 0..len {
            let src = out[(n + len - delay % len) % len];
            if pass == 0 || n >= delay {
                out[n] += 0.34 * src * if pass == 0 { 1.0 } else { 0.5 };
            }
        }
    }
    out.iter_mut().for_each(|v| *v = (*v * 1.1).tanh() * 0.8);
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

    pub fn start(mood: Mood) -> Self {
        let muted = Arc::new(AtomicBool::new(false));
        Self { _stream: Self::open(muted.clone(), mood).ok(), muted }
    }

    fn open(muted: Arc<AtomicBool>, mood: Mood) -> anyhow::Result<cpal::Stream> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let dev = cpal::default_host().default_output_device().ok_or_else(|| anyhow::anyhow!("no output device"))?;
        let cfg = dev.default_output_config()?;
        let (ch, rate) = (cfg.channels() as usize, cfg.sample_rate() as f32);
        let samples = render(mood);
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
