//! Interface preferences, kept in a small JSON file in the user's configuration directory.
//!
//! Location: `%APPDATA%\Kanavos\prefs.json` on Windows, `~/Library/Application Support/Kanavos/prefs.json` on
//! macOS, `$XDG_CONFIG_HOME/kanavos/prefs.json` (default `~/.config/kanavos`) elsewhere. `KANAVOS_CONFIG_DIR`
//! overrides the directory (tests, portable installs). Recovery files live in the data directory next to it.
//!
//! Preferences never contain project content.

use std::path::{Path, PathBuf};

use kanavos_units::Locale;
use kanavos_update::Channel;
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

/// Maximum number of recent files kept.
pub const MAX_RECENT: usize = 10;

/// Persisted interface preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Light or dark theme.
    pub theme: Theme,
    /// Interface language.
    pub locale: Locale,
    /// UI scale factor (1.0 = 100 %).
    pub ui_scale: f32,
    /// Recently opened or saved files, most recent first.
    pub recent: Vec<PathBuf>,
    /// Project tree expanded.
    pub show_tree: bool,
    /// Inspector expanded.
    pub show_inspector: bool,
    /// Two views side by side.
    pub split: bool,
    /// The sample house was opened once (first launch done).
    pub first_run_done: bool,
    /// Autosave interval in seconds (0 disables).
    pub autosave_secs: u64,
    /// Look for a new version when the application starts (sends only the version and operating system to
    /// GitHub; see `docs/updates.md`).
    pub update_check: bool,
    /// Which releases are offered: published releases only, or test builds too.
    pub update_channel: Channel,
    /// A version the person chose to skip; the automatic check does not offer it again.
    pub skipped_version: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: Theme::Light,
            locale: Locale::El,
            ui_scale: 1.0,
            recent: Vec::new(),
            show_tree: true,
            show_inspector: true,
            split: true,
            first_run_done: false,
            autosave_secs: 60,
            update_check: true,
            update_channel: Channel::Stable,
            skipped_version: None,
        }
    }
}

impl Prefs {
    /// Puts a file at the top of the recent list.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    /// Reads preferences from a directory; defaults when missing or unreadable.
    pub fn load_from(dir: &Path) -> Prefs {
        std::fs::read_to_string(dir.join("prefs.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes preferences to a directory, creating it when needed.
    pub fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = dir.join("prefs.json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, dir.join("prefs.json"))
    }

    /// Reads the user's preferences.
    pub fn load() -> Prefs {
        config_dir().map(|d| Prefs::load_from(&d)).unwrap_or_default()
    }

    /// Writes the user's preferences (errors are ignored: preferences are a convenience).
    pub fn save(&self) {
        if let Some(d) = config_dir() {
            let _ = self.save_to(&d);
        }
    }
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The configuration directory of Kanavos, if one can be determined.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = env_dir("KANAVOS_CONFIG_DIR") {
        return Some(d);
    }
    if cfg!(windows) {
        env_dir("APPDATA").map(|d| d.join("Kanavos"))
    } else if cfg!(target_os = "macos") {
        env_dir("HOME").map(|d| d.join("Library").join("Application Support").join("Kanavos"))
    } else {
        env_dir("XDG_CONFIG_HOME")
            .or_else(|| env_dir("HOME").map(|h| h.join(".config")))
            .map(|d| d.join("kanavos"))
    }
}

/// The data directory of Kanavos (recovery files), if one can be determined.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = env_dir("KANAVOS_CONFIG_DIR") {
        return Some(d.join("data"));
    }
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").or_else(|| env_dir("APPDATA")).map(|d| d.join("Kanavos"))
    } else if cfg!(target_os = "macos") {
        config_dir()
    } else {
        env_dir("XDG_DATA_HOME")
            .or_else(|| env_dir("HOME").map(|h| h.join(".local").join("share")))
            .map(|d| d.join("kanavos"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kanavos-prefs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn preferences_round_trip_through_a_file() {
        let dir = temp_dir("rt");
        let mut p = Prefs { theme: Theme::Dark, locale: Locale::En, ui_scale: 1.25, ..Prefs::default() };
        p.add_recent(Path::new("a.kanavos"));
        p.save_to(&dir).unwrap();
        assert_eq!(Prefs::load_from(&dir), p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_broken_files_give_defaults() {
        let dir = temp_dir("broken");
        assert_eq!(Prefs::load_from(&dir), Prefs::default());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("prefs.json"), "{ not json").unwrap();
        assert_eq!(Prefs::load_from(&dir), Prefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_files_are_unique_most_recent_first_and_bounded() {
        let mut p = Prefs::default();
        for i in 0..15 {
            p.add_recent(Path::new(&format!("f{i}.kanavos")));
        }
        p.add_recent(Path::new("f10.kanavos"));
        assert_eq!(p.recent.len(), MAX_RECENT);
        assert_eq!(p.recent.first().map(PathBuf::as_path), Some(Path::new("f10.kanavos")));
        assert_eq!(p.recent.iter().filter(|r| r.as_path() == Path::new("f10.kanavos")).count(), 1);
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_default() {
        let p: Prefs = serde_json::from_str(r#"{"theme":"dark","future_field":1}"#).unwrap();
        assert_eq!(p.theme, Theme::Dark);
        assert!(p.show_tree);
        // Preferences written before updates existed check for updates on the stable channel.
        assert!(p.update_check);
        assert_eq!(p.update_channel, Channel::Stable);
        assert_eq!(p.skipped_version, None);
    }

    #[test]
    fn update_preferences_round_trip() {
        let dir = temp_dir("upd");
        let p = Prefs {
            update_check: false,
            update_channel: Channel::Test,
            skipped_version: Some("0.3.0".into()),
            ..Prefs::default()
        };
        p.save_to(&dir).unwrap();
        let text = std::fs::read_to_string(dir.join("prefs.json")).unwrap();
        assert!(text.contains(r#""update_channel": "test""#), "{text}");
        assert_eq!(Prefs::load_from(&dir), p);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
