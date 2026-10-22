//! In-app test recorder: records the microphone before and after noise removal so the user can play both back and
//! hear what the engine does. Exactly one take exists at any time: starting a new recording deletes the old one
//! (memory and the two WAV files in the data folder), so there is nothing to manage.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;

pub const SAMPLE_RATE: usize = 48_000;
/// Hard cap so a forgotten recording cannot eat memory (60 s of two mono streams is about 23 MB).
pub const MAX_SECONDS: usize = 60;

#[derive(Default)]
struct Take {
    original: Vec<f32>,
    cleaned: Vec<f32>,
}

pub struct Recorder {
    recording: AtomicBool,
    take: Mutex<Take>,
}

impl Recorder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { recording: AtomicBool::new(false), take: Mutex::new(Take::default()) })
    }

    pub fn wav_paths(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join("last_recording_original.wav"), dir.join("last_recording_cleaned.wav"))
    }

    /// Delete the previous take (memory and files) and begin recording.
    pub fn start(&self, dir: &Path) {
        let (o, c) = Self::wav_paths(dir);
        let _ = std::fs::remove_file(o);
        let _ = std::fs::remove_file(c);
        *self.take.lock().unwrap() = Take::default();
        self.recording.store(true, Ordering::SeqCst);
    }

    /// Stop recording and write the two WAV files (overwriting).
    pub fn stop(&self, dir: &Path) -> Result<()> {
        self.recording.store(false, Ordering::SeqCst);
        let take = self.take.lock().unwrap();
        if take.cleaned.is_empty() {
            return Ok(());
        }
        std::fs::create_dir_all(dir)?;
        let (o, c) = Self::wav_paths(dir);
        write_wav(&o, &take.original)?;
        write_wav(&c, &take.cleaned)?;
        Ok(())
    }

    /// Called from the audio thread for every processed block. Never blocks.
    pub fn push(&self, input: &[f32], output: &[f32]) {
        if !self.recording.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut t) = self.take.try_lock() else { return };
        t.original.extend_from_slice(input);
        t.cleaned.extend_from_slice(output);
        if t.cleaned.len() >= MAX_SECONDS * SAMPLE_RATE {
            self.recording.store(false, Ordering::SeqCst);
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    pub fn seconds(&self) -> f32 {
        self.take.lock().map(|t| t.cleaned.len() as f32 / SAMPLE_RATE as f32).unwrap_or(0.0)
    }

    pub fn has_recording(&self) -> bool {
        !self.is_recording() && self.seconds() > 0.2
    }

    /// A copy of one stream of the take, ready to hand to the player.
    pub fn buffer(&self, original: bool) -> Arc<Vec<f32>> {
        let t = self.take.lock().unwrap();
        Arc::new(if original { t.original.clone() } else { t.cleaned.clone() })
    }
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE as u32, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
    }
    w.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qm-rec-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_records_between_start_and_stop_and_saves_both_streams() {
        let dir = tmp("basic");
        let r = Recorder::new();
        r.push(&[0.5; 480], &[0.1; 480]); // not recording: ignored
        assert_eq!(r.seconds(), 0.0);
        r.start(&dir);
        for _ in 0..50 {
            r.push(&[0.5; 480], &[0.1; 480]);
        }
        r.stop(&dir).unwrap();
        assert!((r.seconds() - 0.5).abs() < 1e-3);
        assert!(r.has_recording());
        assert_eq!(r.buffer(true)[0], 0.5);
        assert_eq!(r.buffer(false)[0], 0.1);
        let (o, c) = Recorder::wav_paths(&dir);
        assert!(o.exists() && c.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_recording_deletes_the_old_one() {
        let dir = tmp("replace");
        let r = Recorder::new();
        r.start(&dir);
        r.push(&[0.5; 4800], &[0.1; 4800]);
        r.stop(&dir).unwrap();
        let (o, c) = Recorder::wav_paths(&dir);
        assert!(o.exists());
        r.start(&dir);
        assert!(!o.exists() && !c.exists(), "old files removed as soon as a new recording starts");
        assert_eq!(r.seconds(), 0.0);
        assert!(!r.has_recording());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stops_itself_at_the_length_cap() {
        let dir = tmp("cap");
        let r = Recorder::new();
        r.start(&dir);
        let block = vec![0.0f32; SAMPLE_RATE];
        for _ in 0..(MAX_SECONDS + 5) {
            r.push(&block, &block);
        }
        assert!(!r.is_recording());
        assert!(r.seconds() <= (MAX_SECONDS + 1) as f32);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
