//! Optional self-update from the project's GitHub releases. Everything here is inert unless the user turned
//! "Check for updates automatically" on: with it off the app never opens a network connection. Nothing is logged, and the
//! only request is a plain HTTPS GET of the public release list (GitHub sees the IP address, as for any download; no
//! identifier, version or usage data is sent beyond a fixed User-Agent).
//!
//! Flow: check (at most every 6 h) -> download QuietMic-Setup.exe -> verify its SHA-256 against the release's
//! SHA256SUMS.txt -> remember it in `update/ready.txt` -> install on the next start (or when the user clicks "restart to
//! update"). The installer replaces the app folder in one swap and relaunches the app minimised.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};

const REPO: &str = "officenotfound/noise-cancel";
const INSTALLER_ASSET: &str = "QuietMic-Setup.exe";
const SUMS_ASSET: &str = "SHA256SUMS.txt";
const MAX_INSTALLER_BYTES: u64 = 400 * 1024 * 1024;
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(45);

/// Version of a downloaded, verified update that is waiting to be installed.
static READY: Mutex<Option<String>> = Mutex::new(None);

pub fn ready_version() -> Option<String> {
    READY.lock().ok().and_then(|r| r.clone())
}

/// "v1.2.3" -> [1, 2, 3]. Pre-release tags ("1.2.3-rc1") are not accepted.
pub fn parse_version(s: &str) -> Option<[u64; 3]> {
    let s = s.trim().trim_start_matches('v');
    let mut parts = s.split('.');
    let v = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
    parts.next().is_none().then_some(v)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[derive(Debug, PartialEq)]
struct Release {
    version: String,
    installer_url: String,
    sums_url: String,
}

/// Pick the newest-release facts out of GitHub's `releases/latest` JSON. Only assets hosted under this repository's own
/// release-download path are accepted.
fn parse_release(json: &str) -> Result<Option<Release>> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    if v["draft"].as_bool().unwrap_or(false) || v["prerelease"].as_bool().unwrap_or(false) {
        return Ok(None);
    }
    let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("no tag"))?;
    parse_version(tag).ok_or_else(|| anyhow!("unusable version tag"))?;
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    let asset = |name: &str| -> Option<String> {
        v["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name)).and_then(|a| a["browser_download_url"].as_str()).filter(|u| u.starts_with(&prefix)).map(String::from)
    };
    match (asset(INSTALLER_ASSET), asset(SUMS_ASSET)) {
        (Some(installer_url), Some(sums_url)) => Ok(Some(Release { version: tag.trim_start_matches('v').to_string(), installer_url, sums_url })),
        _ => Ok(None),
    }
}

/// The hex digest listed for `name` in a `sha256sum`-style file.
fn digest_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (hash, file) = (it.next()?, it.next()?.trim_start_matches('*'));
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(windows)]
fn curl_exe() -> PathBuf {
    std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join(r"System32\curl.exe")).unwrap_or_else(|| PathBuf::from("curl.exe"))
}

#[cfg(not(windows))]
fn curl_exe() -> PathBuf {
    PathBuf::from("/usr/bin/curl")
}

/// HTTPS only (also across redirects), TLS 1.2+, with a size cap and a time limit. `out` None returns the body.
fn curl(url: &str, max_bytes: u64, secs: u64, out: Option<&Path>) -> Result<Vec<u8>> {
    if !url.starts_with("https://") {
        bail!("not https");
    }
    let mut cmd = Command::new(curl_exe());
    cmd.args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--tlsv1.2", "-A", "QuietMic-updater", "-m", &secs.to_string(), "--max-filesize", &max_bytes.to_string()]);
    if let Some(o) = out {
        cmd.arg("-o").arg(o);
    }
    cmd.arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let o = cmd.output()?;
    if !o.status.success() {
        bail!("download failed");
    }
    Ok(o.stdout)
}

fn update_dir(dir: &Path) -> PathBuf {
    dir.join("update")
}

/// Look for a newer release and, if there is one, download and verify its installer. Returns its version.
fn check_and_download(dir: &Path, current: &str) -> Result<Option<String>> {
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let json = String::from_utf8(curl(&api, 1_000_000, 30, None)?)?;
    let Some(rel) = parse_release(&json)? else { return Ok(None) };
    if !is_newer(&rel.version, current) {
        return Ok(None);
    }
    let ud = update_dir(dir);
    std::fs::create_dir_all(&ud)?;
    let sums = String::from_utf8(curl(&rel.sums_url, 64 * 1024, 30, None)?)?;
    let want = digest_for(&sums, INSTALLER_ASSET).ok_or_else(|| anyhow!("no checksum for the installer"))?;
    let file = ud.join(format!("QuietMic-Setup-{}.exe", rel.version));
    if sha256_file(&file).ok().as_deref() != Some(want.as_str()) {
        let part = ud.join("download.part");
        let _ = std::fs::remove_file(&part);
        curl(&rel.installer_url, MAX_INSTALLER_BYTES, 1800, Some(&part))?;
        if sha256_file(&part)? != want {
            let _ = std::fs::remove_file(&part);
            bail!("checksum mismatch");
        }
        std::fs::rename(&part, &file)?;
    }
    std::fs::write(ud.join("ready.txt"), format!("{}\n{}\n{}\n", rel.version, want, file.file_name().unwrap().to_string_lossy()))?;
    Ok(Some(rel.version))
}

/// Background worker: does nothing and touches no network while `enabled()` is false.
pub fn start(dir: PathBuf, enabled: impl Fn() -> bool + Send + 'static) {
    // pick up an update downloaded in an earlier run
    if let Some((v, _)) = read_ready(&dir) {
        *READY.lock().unwrap() = Some(v);
    }
    std::thread::spawn(move || {
        let mut last: Option<Instant> = None;
        std::thread::sleep(FIRST_CHECK_AFTER);
        loop {
            if enabled() && last.map_or(true, |t| t.elapsed() >= CHECK_EVERY) {
                last = Some(Instant::now());
                if let Ok(Some(v)) = check_and_download(&dir, env!("CARGO_PKG_VERSION")) {
                    *READY.lock().unwrap() = Some(v);
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

/// Forget and delete anything downloaded (used when the user turns auto-update off).
pub fn clear(dir: &Path) {
    *READY.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(update_dir(dir));
}

/// The verified, still-newer update waiting on disk: (version, installer path).
fn read_ready(dir: &Path) -> Option<(String, PathBuf)> {
    let text = std::fs::read_to_string(update_dir(dir).join("ready.txt")).ok()?;
    let mut l = text.lines();
    let (version, hash, name) = (l.next()?, l.next()?, l.next()?);
    if !is_newer(version, env!("CARGO_PKG_VERSION")) || name.contains(['/', '\\']) {
        return None;
    }
    let path = update_dir(dir).join(name);
    (sha256_file(&path).ok()?.eq_ignore_ascii_case(hash)).then(|| (version.to_string(), path))
}

/// Start the downloaded installer in update mode; the caller must quit right after. False if nothing is ready.
pub fn apply_now(dir: &Path) -> bool {
    let Some((_, installer)) = read_ready(dir) else { return false };
    spawn_installer(&installer)
}

/// At start: install a verified pending update. True means the installer took over and this process should exit.
pub fn apply_pending(dir: &Path) -> bool {
    apply_now(dir)
}

#[cfg(windows)]
fn spawn_installer(installer: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(app_dir) = exe.parent() else { return false };
    // DETACHED_PROCESS | CREATE_NO_WINDOW: the installer outlives this process and shows no window
    Command::new(installer).arg("--update").arg("--dir").arg(app_dir).creation_flags(0x0800_0008).spawn().is_ok()
}

#[cfg(not(windows))]
fn spawn_installer(_installer: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically_and_reject_prereleases() {
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("1.0.0-rc1", "0.1.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
    }

    fn release_json(url_prefix: &str, prerelease: bool) -> String {
        format!(
            r#"{{"tag_name":"v0.2.0","draft":false,"prerelease":{prerelease},"assets":[
            {{"name":"QuietMic-Setup.exe","browser_download_url":"{url_prefix}v0.2.0/QuietMic-Setup.exe"}},
            {{"name":"SHA256SUMS.txt","browser_download_url":"{url_prefix}v0.2.0/SHA256SUMS.txt"}}]}}"#
        )
    }

    #[test]
    fn only_assets_from_this_repositorys_release_path_are_accepted() {
        let good = format!("https://github.com/{REPO}/releases/download/");
        let r = parse_release(&release_json(&good, false)).unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(r.installer_url.starts_with(&good));
        assert_eq!(parse_release(&release_json("https://evil.example/", false)).unwrap(), None);
        assert_eq!(parse_release(&release_json("http://github.com/x/", false)).unwrap(), None);
        assert_eq!(parse_release(&release_json(&good, true)).unwrap(), None);
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn checksum_file_lookup() {
        let h = "ab".repeat(32);
        let sums = format!("{h}  QuietMic-Setup.exe\n{}  other.zip\n", "cd".repeat(32));
        assert_eq!(digest_for(&sums, "QuietMic-Setup.exe").as_deref(), Some(h.as_str()));
        assert_eq!(digest_for(&sums, "missing.exe"), None);
        assert_eq!(digest_for("short  QuietMic-Setup.exe", "QuietMic-Setup.exe"), None);
    }

    #[test]
    fn a_tampered_download_is_not_ready_and_a_good_one_is() {
        let dir = std::env::temp_dir().join(format!("qm-update-test-{}", std::process::id()));
        let ud = update_dir(&dir);
        std::fs::create_dir_all(&ud).unwrap();
        std::fs::write(ud.join("QuietMic-Setup-9.9.9.exe"), b"installer bytes").unwrap();
        let good = sha256_file(&ud.join("QuietMic-Setup-9.9.9.exe")).unwrap();
        std::fs::write(ud.join("ready.txt"), format!("9.9.9\n{good}\nQuietMic-Setup-9.9.9.exe\n")).unwrap();
        assert_eq!(read_ready(&dir).map(|r| r.0).as_deref(), Some("9.9.9"));
        std::fs::write(ud.join("QuietMic-Setup-9.9.9.exe"), b"tampered").unwrap();
        assert!(read_ready(&dir).is_none());
        std::fs::write(ud.join("ready.txt"), format!("9.9.9\n{good}\n..\\evil.exe\n")).unwrap();
        assert!(read_ready(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
