use std::path::Path;

use anyhow::Result;

/// Read a mono (or mixed-down) WAV file as f32 in -1..1, returning (samples, sample rate).
pub fn read_wav(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let all: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    Ok((all.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect(), spec.sample_rate))
}

pub fn write_wav(path: &Path, x: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec)?;
    for v in x {
        w.write_sample(*v)?;
    }
    w.finalize()?;
    Ok(())
}
