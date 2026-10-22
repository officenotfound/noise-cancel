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
}

#[derive(Clone)]
pub struct Options {
    pub dir: PathBuf,
    pub desktop_icon: bool,
    pub autostart: bool,
    pub launch: bool,
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
        std::fs::create_dir_all(&opts.dir)?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let Some(rel) = f.enclosed_name() else { continue };
            let target = opts.dir.join(&rel);
            if f.is_dir() {
                std::fs::create_dir_all(&target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            progress.lock().unwrap().current = rel.to_string_lossy().into_owned();
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)?;
            std::fs::write(&target, &buf)?;
            done += buf.len() as u64;
            progress.lock().unwrap().fraction = done as f32 / total as f32 * 0.9;
        }
        progress.lock().unwrap().current = "registering shortcuts...".into();
        integrate(&opts)?;
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
    let uninstall = opts.dir.join("Uninstall.bat");
    let dir = opts.dir.display().to_string();
    for (name, val) in [
        ("DisplayName", "QuietMic".to_string()),
        ("DisplayVersion", "0.1.0".to_string()),
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
    std::fs::write(
        &uninstall,
        format!(
            "@echo off\r\ntaskkill /im QuietMic.exe /f >nul 2>&1\r\ndel \"{start_menu}\" \"{desktop}\" >nul 2>&1\r\n\
             reg delete \"{key}\" /f >nul 2>&1\r\nreg delete HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run /v QuietMic /f >nul 2>&1\r\n\
             echo QuietMic removed. Settings stay in %APPDATA%\\QuietMic.\r\n(goto) 2>nul & rmdir /s /q \"{dir}\"\r\n"
        ),
    )?;
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
