use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use qm_dsp::{Denoiser, DfnDenoiser, DpdfNet, FastEnhancer, RnnoiseDenoiser};
use serde::{Deserialize, Serialize};

/// Every denoiser the engine can run. Names double as the config-file spelling.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelId {
    Rnnoise,
    Dfn3,
    Dfn3Ll,
    FeT,
    FeB,
    FeS,
    FeM,
    Dp2,
    Dp8,
}

impl ModelId {
    pub fn name(self) -> &'static str {
        match self {
            ModelId::Rnnoise => "rnnoise",
            ModelId::Dfn3 => "dfn3",
            ModelId::Dfn3Ll => "dfn3-ll",
            ModelId::FeT => "fe-t",
            ModelId::FeB => "fe-b",
            ModelId::FeS => "fe-s",
            ModelId::FeM => "fe-m",
            ModelId::Dp2 => "dp2",
            ModelId::Dp8 => "dp8",
        }
    }

    /// Build the denoiser. `atten_lim_db` only affects DeepFilterNet (None = unlimited).
    pub fn build(self, models_dir: &Path, atten_lim_db: Option<f32>) -> Result<Box<dyn Denoiser>> {
        let fe = |s: &str| -> Result<Box<dyn Denoiser>> {
            Ok(Box::new(FastEnhancer::new(s, &models_dir.join(format!("fastenhancer/fastenhancer_{s}_spec.onnx")))?))
        };
        let dp = |n: &str| -> Result<Box<dyn Denoiser>> {
            Ok(Box::new(DpdfNet::new(n, &models_dir.join(format!("dpdfnet/{n}.onnx")))?))
        };
        Ok(match self {
            ModelId::Rnnoise => Box::new(RnnoiseDenoiser::new()),
            ModelId::Dfn3 => Box::new(DfnDenoiser::new("dfn3", &models_dir.join("DeepFilterNet3_onnx.tar.gz"), atten_lim_db)?),
            ModelId::Dfn3Ll => Box::new(DfnDenoiser::new("dfn3-ll", &models_dir.join("DeepFilterNet3_ll_onnx.tar.gz"), atten_lim_db)?),
            ModelId::FeT => fe("t")?,
            ModelId::FeB => fe("b")?,
            ModelId::FeS => fe("s")?,
            ModelId::FeM => fe("m")?,
            ModelId::Dp2 => dp("dpdfnet2_48khz_hr")?,
            ModelId::Dp8 => dp("dpdfnet8_48khz_hr")?,
        })
    }
}

impl FromStr for ModelId {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        use ModelId::*;
        [Rnnoise, Dfn3, Dfn3Ll, FeT, FeB, FeS, FeM, Dp2, Dp8]
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| anyhow!("unknown model {s:?}"))
    }
}
