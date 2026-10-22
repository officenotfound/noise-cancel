//! Voice-print support: speaker embeddings (is this my voice?) and voice-activity detection, from free open models.
//!
//! * `fbank`: Kaldi-compatible 80-band log-mel features, as the WeSpeaker models expect.
//! * `embed`: `SpeakerEmbedder` (WeSpeaker ResNet34 / CAM++ ONNX through tract), cosine scoring, enrollment.

pub mod embed;
pub mod fbank;
pub mod vad;
pub mod wav;

pub use vad::Vad;
pub use embed::{cosine, l2_normalize, SpeakerEmbedder};
