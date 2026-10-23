//! The few things that differ between operating systems: folders, opening links and files, autostart, and the
//! wording for the virtual microphone driver (VB-Cable on Windows, BlackHole on macOS).

use std::path::{Path, PathBuf};

use anyhow::Result;

// ---------------------------------------------------------------- wording

#[cfg(windows)]
pub const CABLE_PRODUCT: &str = "VB-Cable";
#[cfg(target_os = "macos")]
pub const CABLE_PRODUCT: &str = "BlackHole";

/// What the user picks as the microphone in Zoom, Teams, softphones...
#[cfg(windows)]
pub const CABLE_PICK: &str = "CABLE Output";
#[cfg(target_os = "macos")]
pub const CABLE_PICK: &str = "BlackHole 2ch";

#[cfg(windows)]
pub const CABLE_URL: &str = "https://vb-audio.com/Cable/";
#[cfg(target_os = "macos")]
pub const CABLE_URL: &str = "https://existential.audio/blackhole/";

#[cfg(windows)]
pub const DEFAULT_MIC_LABEL: &str = "Windows default microphone";
#[cfg(target_os = "macos")]
pub const DEFAULT_MIC_LABEL: &str = "Mac default microphone";

#[cfg(windows)]
pub const AUTOSTART_LABEL: &str = "Start with Windows";
#[cfg(target_os = "macos")]
pub const AUTOSTART_LABEL: &str = "Start at login";

// ---------------------------------------------------------------- folders

/// Where settings, the log and the last recording live.
#[cfg(windows)]
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("QuietMic")).unwrap_or_else(|| PathBuf::from("QuietMic-data"))
}

#[cfg(target_os = "macos")]
pub fn data_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library/Application Support/QuietMic"))
        .unwrap_or_else(|| PathBuf::from("QuietMic-data"))
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_else(|| PathBuf::from("."))
}

/// Models: `QM_MODELS` override, else next to the program (`models\` on Windows, `Contents/Resources/models` inside a
/// macOS app), else the nearest `models` folder above the program (running from a development build).
pub fn models_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("QM_MODELS") {
        return PathBuf::from(p);
    }
    let exe = exe_dir();
    let mut candidates = vec![exe.join("models"), exe.join("../Resources/models")];
    candidates.extend(exe.ancestors().skip(1).take(4).map(|a| a.join("models")));
    candidates.into_iter().find(|c| c.join("DeepFilterNet3_ll_onnx.tar.gz").exists()).unwrap_or_else(|| exe.join("models"))
}

// ---------------------------------------------------------------- opening things

#[cfg(windows)]
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("explorer.exe").arg(url).spawn();
}

#[cfg(windows)]
pub fn open_text_file(path: &Path) {
    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
}

#[cfg(target_os = "macos")]
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(target_os = "macos")]
pub fn open_text_file(path: &Path) {
    // `-t` opens it in the default text editor.
    let _ = std::process::Command::new("open").arg("-t").arg(path).spawn();
}

// ---------------------------------------------------------------- autostart

/// Start QuietMic hidden in the tray / menu bar when the user logs in.
#[cfg(windows)]
pub fn set_autostart(on: bool, exe: &Path) -> Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
    if on {
        key.set_value("QuietMic", &format!("\"{}\" --minimized", exe.display()))?;
    } else {
        let _ = key.delete_value("QuietMic");
    }
    Ok(())
}

/// A per-user LaunchAgent; macOS runs it at the next login.
#[cfg(target_os = "macos")]
pub fn set_autostart(on: bool, exe: &Path) -> Result<()> {
    let dir = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("Library/LaunchAgents");
    let plist = dir.join("com.quietmic.app.plist");
    if on {
        std::fs::create_dir_all(&dir)?;
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.quietmic.app</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--minimized</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
            exe.display()
        );
        std::fs::write(&plist, body)?;
    } else {
        let _ = std::fs::remove_file(&plist);
    }
    Ok(())
}

// ---------------------------------------------------------------- scheduling

/// Tell macOS this process does latency-critical work, so it is not put to sleep or throttled as a background app
/// (App Nap) while the window is hidden in the menu bar. Without this, real-time audio blocks occasionally stall.
#[cfg(target_os = "macos")]
pub fn keep_awake_for_audio() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
        NSActivityOptions::UserInitiated | NSActivityOptions::LatencyCritical,
        &NSString::from_str("Real-time microphone noise removal"),
    );
    // The activity lasts as long as the token lives: keep it for the whole run.
    std::mem::forget(token);
}

#[cfg(not(target_os = "macos"))]
pub fn keep_awake_for_audio() {}

// ---------------------------------------------------------------- single instance

/// Take the per-user instance lock (an exclusive lock on a file in the data folder; the system drops it when the
/// process ends, even after a crash). `None` means another QuietMic already holds it.
pub fn single_instance(dir: &Path) -> Option<std::fs::File> {
    let f = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(dir.join("instance.lock")).ok()?;
    match f.try_lock() {
        Ok(()) => Some(f),
        Err(std::fs::TryLockError::WouldBlock) => None,
        Err(_) => Some(f), // cannot lock here (odd file system): do not block the start
    }
}

/// A second start leaves this marker; the running instance sees it and shows its window.
pub fn request_show(dir: &Path) {
    let _ = std::fs::write(dir.join("show.request"), b"1");
}

/// True once if another start asked for the window.
pub fn take_show_request(dir: &Path) -> bool {
    std::fs::remove_file(dir.join("show.request")).is_ok()
}

#[cfg(test)]
mod instance_tests {
    use super::*;

    #[test]
    fn second_instance_is_refused_until_the_first_ends() {
        let dir = std::env::temp_dir().join(format!("qm-instance-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = single_instance(&dir).expect("first instance gets the lock");
        assert!(single_instance(&dir).is_none(), "second instance must be refused");
        request_show(&dir);
        assert!(take_show_request(&dir));
        assert!(!take_show_request(&dir));
        drop(first);
        assert!(single_instance(&dir).is_some(), "lock is free again after the first ends");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
