//! Platform-independent DSP for QuietMic. Everything here must be testable on macOS.

mod dfn;
mod rnnoise;

pub use dfn::DfnDenoiser;
pub use rnnoise::RnnoiseDenoiser;

/// Sample rate every denoiser in this crate runs at.
pub const SAMPLE_RATE: u32 = 48_000;
/// Samples per processing frame (10 ms at 48 kHz).
pub const FRAME: usize = 480;

/// A streaming mono 48 kHz denoiser working on fixed 10 ms frames.
pub trait Denoiser {
    /// Human-readable model name.
    fn name(&self) -> &str;
    /// Algorithmic delay (samples) between input and the output carrying it.
    fn algorithmic_delay(&self) -> usize;
    /// `input` and `output` are both `FRAME` samples in [-1, 1].
    fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model(name: &str) -> Option<PathBuf> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models").join(name);
        p.exists().then_some(p) // models are fetched by scripts/fetch_models.sh, not committed
    }

    /// 1 s of a 440 Hz tone at -20 dBFS, fed frame by frame.
    fn run(dn: &mut dyn Denoiser, frames: usize) -> Vec<f32> {
        let mut out = Vec::new();
        let mut buf = [0.0f32; FRAME];
        for f in 0..frames {
            let inp: Vec<f32> = (0..FRAME)
                .map(|i| 0.1 * (2.0 * std::f32::consts::PI * 440.0 * (f * FRAME + i) as f32 / SAMPLE_RATE as f32).sin())
                .collect();
            dn.process_frame(&inp, &mut buf).unwrap();
            out.extend_from_slice(&buf);
        }
        out
    }

    #[test]
    fn rnnoise_outputs_finite_frames() {
        let mut dn = RnnoiseDenoiser::new();
        assert_eq!(dn.algorithmic_delay(), FRAME);
        assert!(run(&mut dn, 100).iter().all(|s| s.is_finite() && s.abs() <= 1.5));
    }

    #[test]
    fn dfn_delays_match_model_configs() {
        for (file, ms) in [("DeepFilterNet3_ll_onnx.tar.gz", 10), ("DeepFilterNet3_onnx.tar.gz", 30)] {
            let Some(p) = model(file) else { continue };
            let dn = DfnDenoiser::new(file, &p, None).unwrap();
            assert_eq!(dn.algorithmic_delay(), ms * SAMPLE_RATE as usize / 1000, "{file}");
        }
    }

    #[test]
    fn dfn_silence_in_silence_out() {
        let Some(p) = model("DeepFilterNet3_ll_onnx.tar.gz") else { return };
        let mut dn = DfnDenoiser::new("ll", &p, None).unwrap();
        let (mut buf, zeros) = ([1.0f32; FRAME], [0.0f32; FRAME]);
        for _ in 0..50 {
            dn.process_frame(&zeros, &mut buf).unwrap();
        }
        assert!(buf.iter().all(|s| s.abs() < 1e-3));
        assert!(run(&mut dn, 50).iter().all(|s| s.is_finite()));
    }
}
