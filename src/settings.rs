//! User settings, stored as JSON in the OS config directory.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Spotify,
    Cover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn is_top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }

    pub fn is_left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorMode {
    Primary,
    Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub theme: Theme,
    pub position: Corner,
    pub duration_secs: u64,
    pub monitor: MonitorMode,
    pub start_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            theme: Theme::System,
            position: if cfg!(target_os = "macos") {
                Corner::TopRight
            } else {
                Corner::BottomRight
            },
            duration_secs: 5,
            monitor: MonitorMode::Primary,
            start_at_login: false,
        }
    }
}

/// The app folder inside the OS config directory. Created on demand.
pub fn config_dir() -> PathBuf {
    let dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("Marquee");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn path() -> PathBuf {
    config_dir().join("settings.json")
}

impl Settings {
    /// Load settings, falling back to defaults when the file is missing or broken.
    pub fn load() -> Self {
        match std::fs::read_to_string(path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("settings file is invalid, using defaults: {e}");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path().with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings {
            theme: Theme::Cover,
            position: Corner::TopLeft,
            ..Settings::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"theme":"spotify"}"#).unwrap();
        assert_eq!(s.theme, Theme::Spotify);
        assert_eq!(s.duration_secs, 5);
        assert!(s.enabled);
    }
}
