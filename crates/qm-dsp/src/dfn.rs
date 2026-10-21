use std::path::Path;

use anyhow::Result;
use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{ArrayView2, ArrayViewMut2};

use crate::{Denoiser, FRAME};

/// DeepFilterNet (v3 or v3 low-latency) through the tract ONNX runtime.
pub struct DfnDenoiser {
    name: String,
    model: DfTract,
    delay: usize,
}

impl DfnDenoiser {
    /// `atten_lim_db`: maximum noise attenuation in dB (upstream default: 100 = effectively unlimited).
    pub fn new(name: &str, model_tar_gz: &Path, atten_lim_db: Option<f32>) -> Result<Self> {
        let params = DfParams::new(model_tar_gz.to_path_buf())?;
        // Thresholds match the upstream `deep-filter` CLI. `RuntimeParams::default()` uses (-10, 30, 20),
        // which skips the DF stage too eagerly and cost ~13 dB SI-SDR on standard DFN3 in our smoke test.
        let mut rp = RuntimeParams::default_with_ch(1).with_thresholds(-15.0, 35.0, 35.0);
        if let Some(db) = atten_lim_db {
            rp = rp.with_atten_lim(db);
        }
        let model = DfTract::new(params, &rp)?;
        assert_eq!(model.hop_size, FRAME, "model hop size must be 10 ms @ 48 kHz");
        let delay = (model.fft_size - model.hop_size) + model.lookahead * model.hop_size;
        Ok(Self { name: name.to_string(), model, delay })
    }
}

impl Denoiser for DfnDenoiser {
    fn name(&self) -> &str {
        &self.name
    }

    fn algorithmic_delay(&self) -> usize {
        self.delay
    }

    fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        let noisy = ArrayView2::from_shape((1, FRAME), input)?;
        let enh = ArrayViewMut2::from_shape((1, FRAME), output)?;
        self.model.process(noisy, enh)?;
        Ok(())
    }
}
