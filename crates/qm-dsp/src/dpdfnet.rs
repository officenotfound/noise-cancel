use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use tract_onnx_new::prelude::*;
use tract_onnx_new::tract_hir::internal::Factoid;
use tract_onnx_new::tract_core::internal::DimLike;

use crate::Denoiser;

type Plan = Arc<TypedRunnableModel>;

/// DPDFNet 48 kHz streaming ONNX (https://github.com/ceva-ip/DPDFNet) run with tract.
///
/// The graph consumes one complex spectrum frame `[1, 1, bins, 2]` plus a flat `state` vector per call and returns
/// the enhanced frame and the next state. The initial state is built from ONNX metadata exactly as the reference
/// `infer_dpdfnet_onnx.py` does. STFT/iSTFT are done here: Vorbis window (power-complementary, so overlap-add at 50%
/// needs no normalisation), causal frames ending at the newest sample.
pub struct DpdfNet {
    name: String,
    plan: Plan,
    win_len: usize,
    hop: usize,
    delay: usize,
    bins: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    history: Vec<f32>,
    acc: Vec<f32>,
    time_buf: Vec<f32>,
    spec_buf: Vec<Complex32>,
    scratch_fwd: Vec<Complex32>,
    scratch_inv: Vec<Complex32>,
    state: Option<TValue>,
}

fn vorbis_window(n: usize) -> Vec<f32> {
    let half = n as f32 / 2.0;
    (0..n)
        .map(|i| {
            let s = (0.5 * PI * (i as f32 + 0.5) / half).sin();
            (0.5 * PI * s * s).sin()
        })
        .collect()
}

fn meta(proto: &tract_onnx_new::pb::ModelProto, key: &str) -> Result<String> {
    proto
        .metadata_props
        .iter()
        .find(|p| p.key == key)
        .map(|p| p.value.clone())
        .ok_or_else(|| anyhow!("ONNX metadata key {key:?} missing"))
}

fn floats(s: &str) -> Result<Vec<f32>> {
    s.split(',').map(|x| x.trim().parse::<f32>().map_err(Into::into)).collect()
}

impl DpdfNet {
    /// `name` is the display name; `onnx` a `dpdfnet*_48khz_hr.onnx`.
    pub fn new(name: &str, onnx: &Path) -> Result<Self> {
        let proto = tract_onnx_new::onnx().proto_model_for_path(onnx).with_context(|| format!("loading {}", onnx.display()))?;
        let mut model = tract_onnx_new::onnx().model_for_proto_model(&proto)?;

        let state_size: usize = meta(&proto, "state_size")?.parse()?;
        let erb_n: usize = meta(&proto, "erb_norm_state_size")?.parse()?;
        let spec_n: usize = meta(&proto, "spec_norm_state_size")?.parse()?;
        let mut init = vec![0.0f32; state_size];
        init[..erb_n].copy_from_slice(&floats(&meta(&proto, "erb_norm_init")?)?);
        init[erb_n..erb_n + spec_n].copy_from_slice(&floats(&meta(&proto, "spec_norm_init")?)?);

        let dims: Vec<_> = model.input_fact(0)?.shape.dims().cloned().collect();
        let bins = dims[dims.len() - 2].concretize().and_then(|d| d.to_usize().ok()).ok_or_else(|| anyhow!("dynamic bin count"))?;
        model.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec![1, 1, bins, 2]))?;
        model.set_input_fact(1, InferenceFact::dt_shape(f32::datum_type(), tvec![state_size]))?;
        let plan = model.into_optimized()?.into_runnable()?;

        let win_len = (bins - 1) * 2;
        let hop = win_len / 2;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(win_len);
        let ifft = planner.plan_fft_inverse(win_len);
        Ok(Self {
            name: name.to_string(),
            plan,
            win_len,
            hop,
            // STFT buffering (1 hop) + ~4 hops of model look-ahead; cross-correlation in qm-bench measures 50 ms
            // for both 48 kHz models, matching the reference script's "output advances by ~4 hops" alignment.
            delay: win_len - hop + 4 * hop,
            bins,
            scratch_fwd: fft.make_scratch_vec(),
            scratch_inv: ifft.make_scratch_vec(),
            fft,
            ifft,
            window: vorbis_window(win_len),
            history: vec![0.0; win_len],
            acc: vec![0.0; win_len],
            time_buf: vec![0.0; win_len],
            spec_buf: vec![Complex32::new(0.0, 0.0); bins],
            state: Some(Tensor::from_shape(&[state_size], &init)?.into_tvalue()),
        })
    }
}

impl Denoiser for DpdfNet {
    fn name(&self) -> &str {
        &self.name
    }

    fn hop(&self) -> usize {
        self.hop
    }

    fn algorithmic_delay(&self) -> usize {
        self.delay
    }

    fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        let (hop, n) = (self.hop, self.win_len);
        self.history.copy_within(hop.., 0);
        self.history[n - hop..].copy_from_slice(input);
        for ((t, x), w) in self.time_buf.iter_mut().zip(&self.history).zip(&self.window) {
            *t = x * w;
        }
        self.fft.process_with_scratch(&mut self.time_buf, &mut self.spec_buf, &mut self.scratch_fwd)?;

        let mut flat = Vec::with_capacity(self.bins * 2);
        for c in &self.spec_buf {
            flat.push(c.re);
            flat.push(c.im);
        }
        let spec_in = tract_ndarray::ArrayD::from_shape_vec(vec![1, 1, self.bins, 2], flat)?.into_tensor();
        let state = self.state.take().ok_or_else(|| anyhow!("missing state"))?;
        let mut out = self.plan.run(tvec!(spec_in.into_tvalue(), state))?;
        let spec_out = out.remove(0);
        self.state = Some(out.remove(0));

        let view = spec_out.to_plain_array_view::<f32>()?;
        let flat = view.as_slice().ok_or_else(|| anyhow!("spec_out is not contiguous"))?;
        for (c, v) in self.spec_buf.iter_mut().zip(flat.chunks_exact(2)) {
            *c = Complex32::new(v[0], v[1]);
        }
        self.spec_buf[0].im = 0.0;
        self.spec_buf[self.bins - 1].im = 0.0;
        self.ifft.process_with_scratch(&mut self.spec_buf, &mut self.time_buf, &mut self.scratch_inv)?;

        let scale = 1.0 / n as f32;
        for ((a, t), w) in self.acc.iter_mut().zip(&self.time_buf).zip(&self.window) {
            *a += t * scale * w;
        }
        output.copy_from_slice(&self.acc[..hop]);
        self.acc.copy_within(hop.., 0);
        self.acc[n - hop..].fill(0.0);
        Ok(())
    }
}
