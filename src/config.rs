//! # Unified TOML configuration — load, save, and one-time migration.
//!
//! All persistent state lives in `~/.config/lazytimezone/config.toml`:
//!
//! ```toml
//! theme = "Nord"
//! favorites = ["Asia/Tokyo", "Europe/London"]
//! ```
//!
//! On first launch after the migration, legacy plain-text files (`theme`
//! and `favorites`) are imported automatically and then deleted.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub favorites: Vec<String>,
}

fn default_theme() -> String {
    "Default".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: default_theme(),
            favorites: vec![],
        }
    }
}

/// Returns `~/.config` by reading `$HOME`.
fn config_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config"))
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("lazytimezone").join("config.toml"))
}

pub fn load() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };

    if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        let cfg = migrate_legacy();
        save(&cfg);
        cfg
    }
}

pub fn save(config: &Config) {
    let Some(path) = config_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = toml::to_string_pretty(config) {
        let _ = fs::write(&path, contents);
    }
}

/// Imports legacy plain-text `theme` and `favorites` files, then removes them.
fn migrate_legacy() -> Config {
    let Some(dir) = config_dir() else {
        return Config::default();
    };
    let app_dir = dir.join("lazytimezone");

    let theme = fs::read_to_string(app_dir.join("theme"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_theme);

    let favorites = fs::read_to_string(app_dir.join("favorites"))
        .ok()
        .map(|contents| {
            contents
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Clean up legacy files.
    let _ = fs::remove_file(app_dir.join("theme"));
    let _ = fs::remove_file(app_dir.join("favorites"));

    Config { theme, favorites }
}
