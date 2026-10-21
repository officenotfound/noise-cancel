use nnnoiseless::DenoiseState;

use crate::{Denoiser, FRAME};

/// RNNoise (via the pure-Rust nnnoiseless port). The model expects 16-bit-range samples.
pub struct RnnoiseDenoiser {
    state: Box<DenoiseState<'static>>,
    scaled_in: [f32; FRAME],
    scaled_out: [f32; FRAME],
}

impl RnnoiseDenoiser {
    pub fn new() -> Self {
        Self { state: DenoiseState::new(), scaled_in: [0.0; FRAME], scaled_out: [0.0; FRAME] }
    }
}

impl Default for RnnoiseDenoiser {
    fn default() -> Self {
        Self::new()
    }
}

impl Denoiser for RnnoiseDenoiser {
    fn name(&self) -> &str {
        "rnnoise"
    }

    fn hop(&self) -> usize {
        FRAME
    }

    fn algorithmic_delay(&self) -> usize {
        FRAME
    }

    fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> anyhow::Result<()> {
        for (d, s) in self.scaled_in.iter_mut().zip(input) {
            *d = s * 32768.0;
        }
        self.state.process_frame(&mut self.scaled_out, &self.scaled_in);
        for (d, s) in output.iter_mut().zip(&self.scaled_out) {
            *d = s / 32768.0;
        }
        Ok(())
    }
}
