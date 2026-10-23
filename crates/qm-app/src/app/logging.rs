use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// No-logs policy: nothing is written to disk unless the app is started with `--log` (or QM_LOG=1) for troubleshooting.
// Without it any log left by an earlier run is deleted at start.
static FILE: OnceLock<Mutex<File>> = OnceLock::new();

pub fn path(dir: &Path) -> PathBuf {
    dir.join("quietmic.log")
}

pub fn init(dir: &Path, enabled: bool) {
    let p = path(dir);
    if !enabled {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = FILE.set(Mutex::new(f));
    }
}

pub fn log(msg: impl AsRef<str>) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    if let Some(f) = FILE.get() {
        if let Ok(mut f) = f.lock() {
            let _ = writeln!(f, "{h:02}:{m:02}:{s:02}Z {}", msg.as_ref());
        }
    }
}
