//! Voice-activity detection with Silero VAD v5 (MIT, the 16 kHz "sequence" export) through tract. Works on 16 kHz
//! audio in chunks of 512 samples (32 ms) and returns the probability that the chunk contains speech. Each call
//! feeds 64 samples of context plus the 512 new samples and carries the LSTM state (h, c).

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tract_onnx::prelude::*;

pub const CHUNK: usize = 512;
const CONTEXT: usize = 64;

pub struct Vad {
    plan: Arc<TypedRunnableModel>,
    h: Vec<f32>,
    c: Vec<f32>,
    context: [f32; CONTEXT],
}

impl Vad {
    pub fn new(onnx: &Path) -> Result<Self> {
        let mut m = tract_onnx::onnx().model_for_path(onnx)?;
        m.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec![1, CONTEXT + CHUNK]))?;
        m.set_input_fact(1, InferenceFact::dt_shape(f32::datum_type(), tvec![1, 1, 128]))?;
        m.set_input_fact(2, InferenceFact::dt_shape(f32::datum_type(), tvec![1, 1, 128]))?;
        let plan = m.into_optimized()?.into_runnable()?;
        Ok(Self { plan, h: vec![0.0; 128], c: vec![0.0; 128], context: [0.0; CONTEXT] })
    }

    pub fn reset(&mut self) {
        self.h.fill(0.0);
        self.c.fill(0.0);
        self.context = [0.0; CONTEXT];
    }

    /// Speech probability of one 512-sample chunk (16 kHz, -1..1).
    pub fn process(&mut self, chunk: &[f32]) -> Result<f32> {
        assert_eq!(chunk.len(), CHUNK);
        let mut x = Vec::with_capacity(CONTEXT + CHUNK);
        x.extend_from_slice(&self.context);
        x.extend_from_slice(chunk);
        self.context.copy_from_slice(&chunk[CHUNK - CONTEXT..]);
        let input = tract_ndarray::Array2::from_shape_vec((1, CONTEXT + CHUNK), x)?.into_tensor();
        let h = tract_ndarray::Array3::from_shape_vec((1, 1, 128), self.h.clone())?.into_tensor();
        let c = tract_ndarray::Array3::from_shape_vec((1, 1, 128), self.c.clone())?.into_tensor();
        let out = self.plan.run(tvec!(input.into_tvalue(), h.into_tvalue(), c.into_tvalue()))?;
        let prob = out[0].to_plain_array_view::<f32>()?.iter().next().copied().unwrap_or(0.0);
        self.h = out[1].to_plain_array_view::<f32>()?.iter().copied().collect();
        self.c = out[2].to_plain_array_view::<f32>()?.iter().copied().collect();
        Ok(prob)
    }
}
