//! Persistent user settings (`%APPDATA%\QuietMic\settings.toml`). Platform independent so it is tested everywhere.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use qm_engine::{ModelId, Preset};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub preset: Preset,
    /// Microphone device id; `None` follows the Windows default microphone.
    pub input_id: Option<String>,
    pub bypass: bool,
    pub autostart: bool,
    /// Check GitHub for new versions and download them in the background. Off unless the user turns it on: with it off the
    /// app never opens a network connection.
    pub auto_update: bool,
    /// Extra noise guard on top of the preset's own default.
    pub guard: bool,
    /// Calibration results, keyed by "<cpu> | <preset>".
    pub calibration: BTreeMap<String, ModelId>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { preset: Preset::Auto, input_id: None, bypass: false, autostart: false, auto_update: false, guard: false, calibration: BTreeMap::new() }
    }
}

impl Settings {
    pub fn calibration_key(cpu: &str, preset: Preset) -> String {
        format!("{cpu} | {}", preset.name())
    }

    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join("settings.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("settings.toml"), toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qm-settings-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn round_trips() {
        let dir = tmp("rt");
        let mut s = Settings { preset: Preset::MaxNoiseRemoval, input_id: Some("{0.0.1.00000000}.{abc}".into()), bypass: true, autostart: true, ..Default::default() };
        s.calibration.insert(Settings::calibration_key("Intel i5", Preset::Auto), ModelId::FeS);
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_file_gives_defaults_and_unknown_fields_are_tolerated() {
        let dir = tmp("bad");
        assert_eq!(Settings::load(&dir), Settings::default());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), "this is not toml ===").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        std::fs::write(dir.join("settings.toml"), "preset = \"light\"\nfuture_option = 3\n").unwrap();
        assert_eq!(Settings::load(&dir).preset, Preset::Light);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
