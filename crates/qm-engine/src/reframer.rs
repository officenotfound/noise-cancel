use std::collections::VecDeque;

use anyhow::Result;
use qm_dsp::Denoiser;

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Lets a denoiser with a fixed `hop` consume and produce blocks of any size.
///
/// Input is queued and fed to the model in hops; output is read from a queue that starts pre-filled with silence
/// so that it can never run dry for a given block size. The prefill is the worst-case deficit
/// `hop - gcd(block, hop)`, which is also the extra latency this adapter adds (0 when block is a multiple of hop).
pub struct Reframer {
    model: Box<dyn Denoiser>,
    inq: VecDeque<f32>,
    outq: VecDeque<f32>,
    hop_in: Vec<f32>,
    hop_out: Vec<f32>,
    prefill: usize,
}

impl Reframer {
    /// `block` is the callback size the audio device will use (samples).
    pub fn new(model: Box<dyn Denoiser>, block: usize) -> Self {
        let hop = model.hop();
        let prefill = hop - gcd(block.max(1), hop);
        Self {
            inq: VecDeque::with_capacity(hop + block),
            outq: std::iter::repeat(0.0).take(prefill).collect(),
            hop_in: vec![0.0; hop],
            hop_out: vec![0.0; hop],
            model,
            prefill,
        }
    }

    pub fn model(&self) -> &dyn Denoiser {
        self.model.as_ref()
    }

    /// Delay added by re-framing alone (samples).
    pub fn added_delay(&self) -> usize {
        self.prefill
    }

    /// Total delay: model algorithmic delay + re-framing.
    pub fn total_delay(&self) -> usize {
        self.model.algorithmic_delay() + self.prefill
    }

    /// Process `input` into `output` (same length). Returns the time the model calls took in seconds, for the governor.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<Vec<(f64, usize)>> {
        let mut timings = Vec::new();
        self.inq.extend(input.iter().copied());
        let hop = self.hop_in.len();
        while self.inq.len() >= hop {
            for s in self.hop_in.iter_mut() {
                *s = self.inq.pop_front().unwrap();
            }
            let t = std::time::Instant::now();
            self.model.process_frame(&self.hop_in, &mut self.hop_out)?;
            timings.push((t.elapsed().as_secs_f64(), hop));
            self.outq.extend(self.hop_out.iter().copied());
        }
        for o in output.iter_mut() {
            // Underrun is impossible by construction; if it ever happens emit silence rather than panic.
            *o = self.outq.pop_front().unwrap_or(0.0);
        }
        Ok(timings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pass-through "model" with a fixed hop, for framing tests.
    struct Pass(usize);
    impl Denoiser for Pass {
        fn name(&self) -> &str {
            "pass"
        }
        fn hop(&self) -> usize {
            self.0
        }
        fn algorithmic_delay(&self) -> usize {
            0
        }
        fn process_frame(&mut self, i: &[f32], o: &mut [f32]) -> Result<()> {
            o.copy_from_slice(i);
            Ok(())
        }
    }

    #[test]
    fn output_is_input_delayed_by_added_delay_for_any_block_size() {
        for (hop, block) in [(480, 480), (512, 480), (320, 480), (200, 480), (512, 160), (480, 441), (320, 1000)] {
            let mut r = Reframer::new(Box::new(Pass(hop)), block);
            let delay = r.added_delay();
            let input: Vec<f32> = (0..block * 40).map(|i| i as f32 + 1.0).collect();
            let mut out = Vec::new();
            for chunk in input.chunks(block) {
                let mut o = vec![0.0; chunk.len()];
                r.process(chunk, &mut o).unwrap();
                out.extend(o);
            }
            for (i, v) in out.iter().enumerate().skip(delay) {
                assert_eq!(*v, input[i - delay], "hop {hop} block {block} at {i}");
            }
        }
    }

    #[test]
    fn matching_block_and_hop_adds_no_delay() {
        assert_eq!(Reframer::new(Box::new(Pass(480)), 480).added_delay(), 0);
        assert_eq!(Reframer::new(Box::new(Pass(320)), 480).added_delay(), 160);
        assert_eq!(Reframer::new(Box::new(Pass(512)), 480).added_delay(), 480);
    }
}
