use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use tract_onnx_new::prelude::*;

use crate::Denoiser;

const N_FFT: usize = 1024;
const BINS: usize = N_FFT / 2 + 1;

type Plan = Arc<TypedRunnableModel>;

/// FastEnhancer 48 kHz (https://github.com/aask1357/fastenhancer, MIT), spectrogram-to-spectrogram ONNX run with tract.
///
/// tract 0.21 cannot run the wav2wav export (its `DFT` node fails shape inference), so STFT/iSTFT are done here:
/// periodic Hann window of 1024, causal frames ending at the newest sample, overlap-add with the exact window-energy
/// normalisation `torch.istft` uses. Algorithmic delay is therefore `N_FFT - hop`.
pub struct FastEnhancer {
    name: String,
    plan: Plan,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    /// Sum of squared window over the frames covering each sample of the oldest `hop` chunk.
    envelope: Vec<f32>,
    /// Last `N_FFT` input samples.
    history: Vec<f32>,
    /// Overlap-add accumulator, `N_FFT` samples.
    acc: Vec<f32>,
    time_buf: Vec<f32>,
    spec_buf: Vec<Complex32>,
    scratch_fwd: Vec<Complex32>,
    scratch_inv: Vec<Complex32>,
    /// `cache_in_*` tensors fed to the next call, in graph input order (after `spec_in`).
    caches: Vec<TValue>,
}

fn hop_for(size: &str) -> Result<usize> {
    Ok(match size {
        "t" | "b" | "s" => 512,
        "m" => 320,
        "l" => 200,
        _ => return Err(anyhow!("unknown FastEnhancer size {size:?} (expected t/b/s/m/l)")),
    })
}

impl FastEnhancer {
    /// `size` is one of t/b/s/m/l; `onnx` is the matching `fastenhancer_<size>_spec.onnx` from release `onnx-48khz-v1`.
    pub fn new(size: &str, onnx: &Path) -> Result<Self> {
        let hop = hop_for(size)?;
        let model = tract_onnx_new::onnx().model_for_path(onnx).with_context(|| format!("loading {}", onnx.display()))?;
        let plan = model.into_optimized()?.into_runnable()?;
        let inputs = plan.model().input_outlets()?.to_vec();
        let caches = inputs[1..]
            .iter()
            .map(|o| {
                let f = plan.model().outlet_fact(*o)?;
                let shape = f.shape.as_concrete().ok_or_else(|| anyhow!("dynamic cache shape"))?;
                Ok(Tensor::zero::<f32>(shape)?.into_tvalue())
            })
            .collect::<Result<Vec<_>>>()?;

        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        let ifft = planner.plan_fft_inverse(N_FFT);
        let window: Vec<f32> = (0..N_FFT).map(|n| 0.5 - 0.5 * (2.0 * PI * n as f32 / N_FFT as f32).cos()).collect();
        let envelope = (0..hop)
            .map(|p| (0..).map(|j| p + j * hop).take_while(|&i| i < N_FFT).map(|i| window[i] * window[i]).sum())
            .collect();
        Ok(Self {
            name: format!("fastenhancer-{size}"),
            plan,
            hop,
            scratch_fwd: fft.make_scratch_vec(),
            scratch_inv: ifft.make_scratch_vec(),
            fft,
            ifft,
            window,
            envelope,
            history: vec![0.0; N_FFT],
            acc: vec![0.0; N_FFT],
            time_buf: vec![0.0; N_FFT],
            spec_buf: vec![Complex32::new(0.0, 0.0); BINS],
            caches,
            // keep struct fields grouped by purpose above
        })
    }
}

impl Denoiser for FastEnhancer {
    fn name(&self) -> &str {
        &self.name
    }

    fn hop(&self) -> usize {
        self.hop
    }

    fn algorithmic_delay(&self) -> usize {
        N_FFT - self.hop
    }

    fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        let hop = self.hop;
        self.history.copy_within(hop.., 0);
        self.history[N_FFT - hop..].copy_from_slice(input);

        for ((t, x), w) in self.time_buf.iter_mut().zip(&self.history).zip(&self.window) {
            *t = x * w;
        }
        self.fft.process_with_scratch(&mut self.time_buf, &mut self.spec_buf, &mut self.scratch_fwd)?;

        // spec_in: [1, BINS, 1, 2] (re, im)
        let mut flat = Vec::with_capacity(BINS * 2);
        for c in &self.spec_buf {
            flat.push(c.re);
            flat.push(c.im);
        }
        let spec_in = tract_ndarray::ArrayD::from_shape_vec(vec![1, BINS, 1, 2], flat)?.into_tensor();
        let mut args: TVec<TValue> = tvec!(spec_in.into_tvalue());
        args.extend(self.caches.drain(..));
        let mut out = self.plan.run(args)?;
        let spec_out = out.remove(0);
        self.caches = out.into_iter().collect();

        let spec_view = spec_out.to_plain_array_view::<f32>()?;
        let spec_flat = spec_view.as_slice().ok_or_else(|| anyhow!("spec_out is not contiguous"))?;
        for (c, v) in self.spec_buf.iter_mut().zip(spec_flat.chunks_exact(2)) {
            *c = Complex32::new(v[0], v[1]);
        }
        // realfft requires purely real DC and Nyquist bins for the inverse.
        self.spec_buf[0].im = 0.0;
        self.spec_buf[BINS - 1].im = 0.0;
        self.ifft.process_with_scratch(&mut self.spec_buf, &mut self.time_buf, &mut self.scratch_inv)?;

        let scale = 1.0 / N_FFT as f32; // realfft's inverse is unnormalised
        for ((a, t), w) in self.acc.iter_mut().zip(&self.time_buf).zip(&self.window) {
            *a += t * scale * w;
        }
        for ((o, a), e) in output.iter_mut().zip(&self.acc).zip(&self.envelope) {
            *o = a / e;
        }
        self.acc.copy_within(hop.., 0);
        self.acc[N_FFT - hop..].fill(0.0);
        Ok(())
    }
}
