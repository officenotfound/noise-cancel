//! Speaker embeddings from the WeSpeaker ONNX models (ResNet34: 256 numbers, CAM++: 512), through tract.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use tract_onnx::prelude::*;

use crate::fbank::{mean_normalize, Fbank, NUM_BINS};

type Plan = Arc<TypedRunnableModel>;

pub struct SpeakerEmbedder {
    base: InferenceModel,
    /// One optimised plan per input length (in frames); the live gate uses a single window length.
    plans: HashMap<usize, Plan>,
    fbank: Fbank,
    pub dim: usize,
}

impl SpeakerEmbedder {
    pub fn new(onnx: &Path) -> Result<Self> {
        let proto = tract_onnx::onnx().proto_model_for_path(onnx)?;
        let dim = proto.metadata_props.iter().find(|p| p.key == "output_dim").and_then(|p| p.value.parse().ok()).ok_or_else(|| anyhow!("model has no output_dim metadata"))?;
        let base = tract_onnx::onnx().model_for_proto_model(&proto)?;
        Ok(Self { base, plans: HashMap::new(), fbank: Fbank::new(), dim })
    }

    fn plan(&mut self, frames: usize) -> Result<Plan> {
        if let Some(p) = self.plans.get(&frames) {
            return Ok(p.clone());
        }
        let mut m = self.base.clone();
        m.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec![1, frames, NUM_BINS]))?;
        let plan = m.into_optimized()?.into_runnable()?;
        self.plans.insert(frames, plan.clone());
        Ok(plan)
    }

    /// Embedding (L2-normalised) of 16 kHz mono audio in -1..1. Needs at least 0.5 s.
    pub fn embed(&mut self, samples_16k: &[f32]) -> Result<Vec<f32>> {
        let mut feats = self.fbank.compute(samples_16k);
        let frames = feats.len() / NUM_BINS;
        if frames < 48 {
            return Err(anyhow!("audio too short for an embedding ({frames} frames)"));
        }
        mean_normalize(&mut feats);
        let plan = self.plan(frames)?;
        let input = tract_ndarray::Array3::from_shape_vec((1, frames, NUM_BINS), feats)?.into_tensor();
        let out = plan.run(tvec!(input.into_tvalue()))?;
        let view = out[0].to_plain_array_view::<f32>()?;
        let mut v: Vec<f32> = view.iter().copied().collect();
        l2_normalize(&mut v);
        Ok(v)
    }
}

pub fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter_mut().for_each(|x| *x /= n);
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let (na, nb) = (a.iter().map(|x| x * x).sum::<f32>().sqrt(), b.iter().map(|x| x * x).sum::<f32>().sqrt());
    dot / (na * nb).max(1e-12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_properties() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 2.0], &[-1.0, -2.0]) + 1.0).abs() < 1e-6);
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
    }
}
