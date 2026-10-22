//! vad_check model.onnx file.wav : print the speech probability per 32 ms chunk (compact) and a summary.
use std::path::Path;

use qm_voice::vad::{Vad, CHUNK};
use qm_voice::wav::read_wav;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mut vad = Vad::new(Path::new(&a[1]))?;
    let (x, sr) = read_wav(Path::new(&a[2]))?;
    assert_eq!(sr, 16_000);
    // 1 s of silence, the file, 1 s of silence
    let mut s = vec![0.0f32; 16_000];
    s.extend_from_slice(&x);
    s.extend(vec![0.0f32; 16_000]);
    let probs: Vec<f32> = s.chunks_exact(CHUNK).map(|c| vad.process(c).unwrap()).collect();
    let line: String = probs.iter().map(|p| match *p { p if p > 0.8 => '#', p if p > 0.5 => '+', p if p > 0.2 => '.', _ => ' ' }).collect();
    println!("[{line}]");
    let lead = &probs[..30];
    let tail = &probs[probs.len() - 30..];
    println!("silence before: max {:.3}   silence after: max {:.3}   speech: mean {:.2}", lead.iter().cloned().fold(0.0, f32::max), tail.iter().cloned().fold(0.0, f32::max), probs[32..probs.len() - 32].iter().sum::<f32>() / (probs.len() - 64) as f32);
    Ok(())
}
