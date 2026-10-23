//! Unpacks the embedded app and wires it into Windows (Start Menu, optional desktop icon and autostart, an entry in
//! "Installed apps" and an uninstaller). Per-user: nothing here needs administrator rights.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

#[derive(Clone, Default)]
pub struct Progress {
    pub fraction: f32,
    pub current: String,
    pub done: bool,
    pub error: Option<String>,
    /// Something optional went wrong (for example the VB-Cable driver); the install itself is fine.
    pub warning: Option<String>,
}

#[derive(Clone)]
pub struct Options {
    pub dir: PathBuf,
    pub desktop_icon: bool,
    pub autostart: bool,
    pub launch: bool,
    pub vbcable: bool,
    /// Some(on) records the user's choice in settings.toml; None leaves the setting alone (updates).
    pub auto_update: Option<bool>,
    /// Unpack next to the install folder and swap it in at the end, so a failure never leaves a half-updated app.
    pub staged: bool,
}

pub fn default_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join("QuietMic")).unwrap_or_else(|| PathBuf::from("QuietMic"))
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join("QuietMic-installer-test")
    }
}

pub fn payload_size() -> usize {
    PAYLOAD.len()
}

pub fn run(opts: Options, progress: Arc<Mutex<Progress>>) {
    let result = (|| -> Result<()> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD))?;
        if zip.is_empty() {
            return Err(anyhow!("this installer was built without the QuietMic files"));
        }
        let total: u64 = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.size())).sum::<u64>().max(1);
        let mut done = 0u64;
        let target = if opts.staged { opts.dir.with_extension("update") } else { opts.dir.clone() };
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target)?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let Some(rel) = f.enclosed_name() else { continue };
            let dest = target.join(&rel);
            if f.is_dir() {
                std::fs::create_dir_all(&dest)?;
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            progress.lock().unwrap().current = rel.to_string_lossy().into_owned();
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)?;
            std::fs::write(&dest, &buf)?;
            done += buf.len() as u64;
            progress.lock().unwrap().fraction = done as f32 / total as f32 * 0.9;
        }
        if opts.staged {
            progress.lock().unwrap().current = "swapping in the new version...".into();
            swap_in(&target, &opts.dir)?;
        }
        if let Some(on) = opts.auto_update {
            set_auto_update(on);
        }
        progress.lock().unwrap().current = "registering shortcuts...".into();
        integrate(&opts)?;
        progress.lock().unwrap().fraction = 0.93;
        if opts.vbcable {
            progress.lock().unwrap().current = "installing the VB-Cable virtual microphone (approve the Windows prompt)...".into();
            if let Err(e) = install_vbcable() {
                progress.lock().unwrap().warning = Some(format!("VB-Cable was not installed ({e}). Install it from vb-audio.com/Cable"));
            }
        }
        let mut p = progress.lock().unwrap();
        p.fraction = 1.0;
        p.current = "all done".into();
        Ok(())
    })();
    let mut p = progress.lock().unwrap();
    match result {
        Ok(()) => p.done = true,
        Err(e) => p.error = Some(format!("{e:#}")),
    }
}

#[cfg(not(windows))]
fn integrate(_opts: &Options) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn integrate(opts: &Options) -> Result<()> {
    use std::process::Command;
    // tests use their own folder and must not touch the shortcuts and registry entries of a real install
    if std::env::var_os("QM_NO_INTEGRATION").is_some() {
        return Ok(());
    }
    let exe = opts.dir.join("QuietMic.exe");
    let start_menu = std::env::var("APPDATA")? + r"\Microsoft\Windows\Start Menu\Programs\QuietMic.lnk";
    let desktop = std::env::var("USERPROFILE")? + r"\Desktop\QuietMic.lnk";
    let ps_lnk = |lnk: &str| {
        format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}');$s.TargetPath='{exe}';$s.WorkingDirectory='{dir}';$s.Save()",
            exe = exe.display(),
            dir = opts.dir.display()
        )
    };
    let mut script = ps_lnk(&start_menu);
    if opts.desktop_icon {
        script.push(';');
        script.push_str(&ps_lnk(&desktop));
    }
    let hide = 0x0800_0000u32; // CREATE_NO_WINDOW
    use std::os::windows::process::CommandExt;
    let _ = Command::new("powershell").args(["-NoProfile", "-Command", &script]).creation_flags(hide).status();

    let reg = |args: &[&str]| {
        let _ = Command::new("reg").args(args).creation_flags(hide).status();
    };
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\QuietMic";
    let uninstall = opts.dir.join("Uninstall.exe");
    let dir = opts.dir.display().to_string();
    for (name, val) in [
        ("DisplayName", "QuietMic".to_string()),
        ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
        ("Publisher", "QuietMic".to_string()),
        ("InstallLocation", dir.clone()),
        ("DisplayIcon", exe.display().to_string()),
        ("UninstallString", format!("\"{}\"", uninstall.display())),
    ] {
        reg(&["add", key, "/v", name, "/t", "REG_SZ", "/d", &val, "/f"]);
    }
    if opts.autostart {
        reg(&["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "QuietMic", "/t", "REG_SZ", "/d", &format!("\"{}\" --minimized", exe.display()), "/f"]);
    }
    Ok(())
}

pub fn launch(dir: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new(dir.join("QuietMic.exe")).current_dir(dir).spawn();
    }
    #[cfg(not(windows))]
    let _ = dir;
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = url;
}

const VBCABLE_URL: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";

/// Is the VB-Cable driver already on this PC?
pub fn cable_installed() -> bool {
    #[cfg(windows)]
    {
        let sys = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        std::fs::read_dir(sys.join(r"System32\drivers")).map(|d| d.filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().to_lowercase().starts_with("vbaudio_cable"))).unwrap_or(false)
    }
    #[cfg(not(windows))]
    false
}

/// Download VB-Audio's own driver pack from their site and run their setup (it asks Windows for administrator rights
/// and for permission to install the driver). VB-CABLE is donationware that may not be bundled, so it is never shipped here.
#[cfg(windows)]
fn install_vbcable() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let hide = 0x0800_0000u32; // CREATE_NO_WINDOW
    let tmp = std::env::temp_dir().join("quietmic-vbcable");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let zip = tmp.join("pack.zip");
    let ok = Command::new("curl.exe").args(["-fsSL", "-m", "120", "-o"]).arg(&zip).arg(VBCABLE_URL).creation_flags(hide).status()?.success();
    if !ok {
        return Err(anyhow!("download failed"));
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip)?)?;
    archive.extract(&tmp)?;
    let arm = std::env::var("PROCESSOR_ARCHITECTURE").map(|a| a.eq_ignore_ascii_case("ARM64")).unwrap_or(false);
    let want = if arm { "arm64" } else { "x64" };
    let setup = std::fs::read_dir(&tmp)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
            n.contains("setup") && n.contains(want) && n.ends_with(".exe")
        })
        .ok_or_else(|| anyhow!("setup program not found in the download"))?;
    let script = format!("Start-Process -FilePath '{}' -ArgumentList '-i','-h' -Verb RunAs -Wait", setup.display());
    let status = Command::new("powershell").args(["-NoProfile", "-Command", &script]).creation_flags(hide).status()?;
    if !status.success() {
        return Err(anyhow!("setup was cancelled"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_vbcable() -> Result<()> {
    Err(anyhow!("Windows only"))
}

/// Replace `dir` with the freshly unpacked `new`; the old version is put back if the swap fails.
fn swap_in(new: &Path, dir: &Path) -> Result<()> {
    let old = dir.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    let had_old = dir.exists();
    if had_old {
        std::fs::rename(dir, &old).map_err(|e| anyhow!("could not move the old version aside ({e}); is QuietMic still running?"))?;
    }
    if let Err(e) = std::fs::rename(new, dir) {
        if had_old {
            let _ = std::fs::rename(&old, dir);
        }
        return Err(anyhow!("could not put the new version in place ({e})"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Record the auto-update choice in QuietMic's settings file (created if missing; other settings are kept).
fn set_auto_update(on: bool) {
    #[cfg(windows)]
    {
        let Some(appdata) = std::env::var_os("APPDATA") else { return };
        let dir = PathBuf::from(appdata).join("QuietMic");
        let path = dir.join("settings.toml");
        let _ = std::fs::create_dir_all(&dir);
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let mut text: String = old.lines().filter(|l| !l.trim_start().starts_with("auto_update")).map(|l| format!("{l}\n")).collect();
        // top-level keys must come before any [table] in TOML
        text.insert_str(0, &format!("auto_update = {on}\n"));
        let _ = std::fs::write(&path, text);
    }
    #[cfg(not(windows))]
    let _ = on;
}

/// `--update`: wait for the app to exit, swap in the new version, start it again minimised. Settings (in AppData) stay.
pub fn update_in_place(dir: &Path) -> bool {
    stop_app();
    let progress = Arc::new(Mutex::new(Progress::default()));
    run(Options { dir: dir.to_path_buf(), desktop_icon: false, autostart: false, launch: false, vbcable: false, auto_update: None, staged: true }, progress.clone());
    let ok = progress.lock().unwrap().done;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new(dir.join("QuietMic.exe")).arg("--minimized").current_dir(dir).creation_flags(0x0800_0008).spawn();
    }
    ok
}

fn stop_app() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // give the app a moment to exit by itself, then make sure
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = std::process::Command::new("taskkill").args(["/im", "QuietMic.exe", "/f"]).creation_flags(0x0800_0000).status();
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
}
