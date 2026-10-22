//! Playback state shared between the UI and an audio-device thread: a buffer, a position and play/pause flags.
//! Platform independent so it is unit-tested everywhere; the device threads (WASAPI / CoreAudio) just call `fill`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct PlayShared {
    samples: Mutex<Arc<Vec<f32>>>,
    pos: AtomicUsize,
    playing: AtomicBool,
    finished: AtomicBool,
}

impl PlayShared {
    pub fn new(samples: Arc<Vec<f32>>) -> Arc<Self> {
        Arc::new(Self { samples: Mutex::new(samples), pos: AtomicUsize::new(0), playing: AtomicBool::new(false), finished: AtomicBool::new(false) })
    }

    /// Called from the audio thread: writes the next `out.len()` samples (silence when paused or finished).
    pub fn fill(&self, out: &mut [f32]) {
        out.fill(0.0);
        if !self.playing.load(Ordering::Relaxed) {
            return;
        }
        // try_lock: never block the audio thread; a missed block is just a moment of silence.
        let Ok(samples) = self.samples.try_lock() else { return };
        let pos = self.pos.load(Ordering::Relaxed);
        let n = out.len().min(samples.len().saturating_sub(pos));
        out[..n].copy_from_slice(&samples[pos..pos + n]);
        let end = pos + n;
        self.pos.store(end, Ordering::Relaxed);
        if end >= samples.len() {
            self.playing.store(false, Ordering::Relaxed);
            self.finished.store(true, Ordering::Relaxed);
        }
    }

    pub fn play(&self) {
        if self.finished.load(Ordering::Relaxed) {
            self.pos.store(0, Ordering::Relaxed);
            self.finished.store(false, Ordering::Relaxed);
        }
        self.playing.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> usize {
        self.pos.load(Ordering::Relaxed)
    }

    /// Swap the buffer while keeping the position (switching between the cleaned and the original take).
    pub fn switch_samples(&self, samples: Arc<Vec<f32>>) {
        *self.samples.lock().unwrap() = samples;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Arc<Vec<f32>> {
        Arc::new((0..n).map(|i| i as f32 + 1.0).collect())
    }

    #[test]
    fn silent_until_played_then_streams_in_order() {
        let p = PlayShared::new(ramp(10));
        let mut out = [9.0; 4];
        p.fill(&mut out);
        assert_eq!(out, [0.0; 4]);
        p.play();
        p.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        p.fill(&mut out);
        assert_eq!(out, [5.0, 6.0, 7.0, 8.0]);
        assert_eq!(p.position(), 8);
    }

    #[test]
    fn pause_holds_position_and_resumes() {
        let p = PlayShared::new(ramp(10));
        p.play();
        let mut out = [0.0; 3];
        p.fill(&mut out);
        p.pause();
        p.fill(&mut out);
        assert_eq!(out, [0.0; 3]);
        assert_eq!(p.position(), 3);
        p.play();
        p.fill(&mut out);
        assert_eq!(out, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn ends_with_partial_block_then_finished_and_replay_restarts() {
        let p = PlayShared::new(ramp(5));
        p.play();
        let mut out = [9.0; 4];
        p.fill(&mut out);
        p.fill(&mut out);
        assert_eq!(out, [5.0, 0.0, 0.0, 0.0]);
        assert!(p.is_finished() && !p.is_playing());
        p.play();
        assert_eq!(p.position(), 0);
        p.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn switching_buffers_keeps_the_position() {
        let p = PlayShared::new(ramp(10));
        p.play();
        let mut out = [0.0; 3];
        p.fill(&mut out);
        p.switch_samples(Arc::new(vec![-1.0; 10]));
        p.fill(&mut out);
        assert_eq!(out, [-1.0; 3]);
        assert_eq!(p.position(), 6);
    }
}
