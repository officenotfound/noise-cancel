//! Human-readable performance report written after calibration (opened from the tray menu).

use qm_engine::governor::Timing;
use qm_engine::{ModelId, Preset};

pub fn cpu_name() -> String {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown CPU".into())
}

pub fn render(preset: Preset, timings: &[Timing], picked: ModelId) -> String {
    let mut s = String::new();
    s.push_str(&format!("QuietMic performance report\nCPU: {}\nLogical processors: {}\nPreset: {}\n\n", cpu_name(),
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0), preset.name()));
    s.push_str("Each model was timed on this PC (single thread). 'hop' is the real-time deadline per call;\n");
    s.push_str("mean/p99 well below the hop means the model runs comfortably. Models far above it are skipped.\n\n");
    s.push_str(&format!("{:<9} {:>8} {:>11} {:>9} {:>9} {:>10}\n", "model", "hop ms", "delay ms", "mean ms", "p99 ms", "mean %hop"));
    for t in timings {
        s.push_str(&format!("{:<9} {:>8.1} {:>11.1} {:>9.2} {:>9.2} {:>9.0}%\n", t.model.name(), t.hop_ms, t.algorithmic_ms, t.mean_ms, t.p99_ms, 100.0 * t.mean_frac()));
    }
    s.push_str(&format!("\nChosen for this preset: {} (rule: p99 <= 40% of hop and mean <= 25%)\n", picked.name()));
    s
}
