//! Removes QuietMic: stops it, deletes the shortcuts, the autostart entry, the "Installed apps" entry, optionally the
//! settings, and finally the install folder. Everything is per-user, like the installer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

#[derive(Clone, Default)]
pub struct Progress {
    pub fraction: f32,
    pub current: String,
    pub done: bool,
    pub error: Option<String>,
    /// Things that could not be removed (for example a file still in use); the rest was removed.
    pub leftovers: Vec<String>,
}

#[derive(Clone)]
pub struct Options {
    pub dir: PathBuf,
    pub remove_settings: bool,
}

/// The folder this uninstaller belongs to: `--dir X` (set when it re-launched itself from the temp folder) or the folder
/// of its own executable.
pub fn install_dir(arg: Option<String>) -> PathBuf {
    arg.map(PathBuf::from).unwrap_or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default())
}

/// Only ever delete a folder that really is a QuietMic install, never a wrong path.
fn looks_like_install(dir: &Path) -> bool {
    dir.components().count() >= 3 && (dir.join("QuietMic.exe").exists() || dir.join("QuietMic").exists() || dir.join("Uninstall.exe").exists())
}

/// A running exe cannot delete its own folder on Windows: copy this uninstaller to the temp folder, start the copy with
/// `--dir`, and let this process exit. Returns true when the copy took over.
pub fn relaunch_from_temp(dir: &Path, extra: &[&str], wait: bool) -> bool {
    let Ok(me) = std::env::current_exe() else { return false };
    if me.parent() != Some(dir) {
        return false;
    }
    let copy = std::env::temp_dir().join(format!("QuietMic-Uninstall-{}.exe", std::process::id()));
    if std::fs::copy(&me, &copy).is_err() {
        return false;
    }
    let mut cmd = std::process::Command::new(&copy);
    cmd.arg("--dir").arg(dir).args(extra);
    if wait {
        cmd.status().map(|s| s.success()).unwrap_or(false)
    } else {
        cmd.spawn().is_ok()
    }
}

#[cfg(windows)]
fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                files_under(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

pub fn run(opts: Options, progress: Arc<Mutex<Progress>>) {
    let set = |f: f32, msg: &str| {
        let mut p = progress.lock().unwrap();
        p.fraction = f;
        p.current = msg.to_string();
    };
    let result = (|| -> Result<()> {
        if !looks_like_install(&opts.dir) {
            return Err(anyhow!("{} does not look like a QuietMic install, nothing was removed", opts.dir.display()));
        }
        set(0.02, "stopping QuietMic...");
        platform::stop_app();
        std::thread::sleep(std::time::Duration::from_millis(800));
        set(0.08, "removing shortcuts and registry entries...");
        platform::remove_integration();
        if opts.remove_settings {
            set(0.12, "removing settings and recordings...");
            if let Some(d) = platform::settings_dir() {
                let _ = std::fs::remove_dir_all(d);
            }
        }
        let mut files = Vec::new();
        files_under(&opts.dir, &mut files);
        let total = files.len().max(1);
        for (i, f) in files.iter().enumerate() {
            let rel = f.strip_prefix(&opts.dir).unwrap_or(f).to_string_lossy().into_owned();
            set(0.15 + 0.8 * i as f32 / total as f32, &rel);
            // an app that is still shutting down may hold a file for a moment
            let mut tries = 0;
            while let Err(e) = std::fs::remove_file(f) {
                tries += 1;
                if tries >= 5 {
                    progress.lock().unwrap().leftovers.push(format!("{rel} ({e})"));
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
        set(0.97, "removing folders...");
        remove_empty_dirs(&opts.dir);
        set(1.0, "all gone");
        Ok(())
    })();
    let mut p = progress.lock().unwrap();
    match result {
        Ok(()) => p.done = true,
        Err(e) => p.error = Some(format!("{e:#}")),
    }
}

fn remove_empty_dirs(dir: &Path) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            if e.path().is_dir() {
                remove_empty_dirs(&e.path());
            }
        }
    }
    let _ = std::fs::remove_dir(dir);
}

/// Last thing the temp copy does: delete itself once this process has ended.
pub fn delete_self_later() {
    #[cfg(windows)]
    if let Ok(me) = std::env::current_exe() {
        if me.starts_with(std::env::temp_dir()) {
            let line = format!("ping -n 4 127.0.0.1 >nul & del /f /q \"{}\"", me.display());
            let _ = hidden(std::process::Command::new("cmd").args(["/c", &line])).spawn();
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;
    use std::process::Command;

    use super::hidden;

    pub fn stop_app() {
        if std::env::var_os("QM_NO_INTEGRATION").is_some() {
            return;
        }
        let _ = hidden(Command::new("taskkill").args(["/im", "QuietMic.exe", "/f"])).status();
    }

    pub fn settings_dir() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("QuietMic"))
    }

    pub fn remove_integration() {
        if std::env::var_os("QM_NO_INTEGRATION").is_some() {
            return;
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let _ = std::fs::remove_file(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\QuietMic.lnk"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let _ = std::fs::remove_file(PathBuf::from(profile).join(r"Desktop\QuietMic.lnk"));
        }
        let _ = hidden(Command::new("reg").args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\QuietMic", "/f"])).status();
        let _ = hidden(Command::new("reg").args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "QuietMic", "/f"])).status();
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::PathBuf;

    pub fn stop_app() {}

    pub fn settings_dir() -> Option<PathBuf> {
        None
    }

    pub fn remove_integration() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_an_install_and_refuses_a_foreign_folder() {
        let base = std::env::temp_dir().join(format!("qm-uninstall-test-{}", std::process::id()));
        let dir = base.join("a").join("QuietMic");
        std::fs::create_dir_all(dir.join("models/x")).unwrap();
        std::fs::write(dir.join("QuietMic"), b"x").unwrap();
        std::fs::write(dir.join("models/x/m.onnx"), b"m").unwrap();
        let progress = Arc::new(Mutex::new(Progress::default()));
        run(Options { dir: dir.clone(), remove_settings: false }, progress.clone());
        assert!(progress.lock().unwrap().done, "{:?}", progress.lock().unwrap().error);
        assert!(!dir.exists());

        let foreign = base.join("a").join("Documents");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("keep.txt"), b"keep").unwrap();
        let progress = Arc::new(Mutex::new(Progress::default()));
        run(Options { dir: foreign.clone(), remove_settings: true }, progress.clone());
        assert!(progress.lock().unwrap().error.is_some());
        assert!(foreign.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
