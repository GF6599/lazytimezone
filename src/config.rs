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
//!
//! ## API shape
//!
//! - [`try_load`] / [`try_save`] take an explicit path and surface
//!   [`io::Result`] so a caller can choose to abort, warn, or fall back
//!   to defaults without auto-clobbering a malformed user file.
//! - [`default_path`] exposes the canonical path so a caller can format
//!   diagnostics or feed the rich API.

use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Non-fatal issue encountered while loading the config.
///
/// Currently used for soft-failures like an unknown theme label, which
/// shouldn't abort startup but should be visible to the user.
#[derive(Debug, Clone)]
pub struct LoadWarning(pub String);

/// Returns the base config directory, respecting `$XDG_CONFIG_HOME`
/// with a fallback to `$HOME/.config`.
fn config_dir() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("lazytimezone").join("config.toml"))
}

/// Canonical on-disk config path, or `None` if neither `$XDG_CONFIG_HOME`
/// nor `$HOME` is set.
///
/// Exposed so a future caller of [`try_load`] can format diagnostics
/// against the same path the loader is reading from. Wired up by a
/// later wave; the `allow(dead_code)` silences the interim warning.
#[allow(dead_code)]
pub fn default_path() -> Option<PathBuf> {
    config_path()
}

/// Path-aware loader that distinguishes the three states a caller cares
/// about: file missing (defaults, no warnings), file present and parsed
/// (config + any soft warnings), file present but unreadable / malformed
/// (error).
///
/// On a parse error the caller must NOT auto-save — that would clobber
/// the user's broken file before they have a chance to fix it.
///
/// When the path doesn't exist the legacy plain-text migration still
/// runs so first-launch upgrades behave the same as before.
pub fn try_load(path: &Path) -> io::Result<(Config, Vec<LoadWarning>)> {
    if !path.exists() {
        let cfg = migrate_legacy();
        // Migration is a one-shot upgrade — persist the result so legacy
        // files don't get re-read on the next launch. Errors here are
        // surfaced as a LoadWarning so the TUI status bar can display
        // them; stderr is invisible in alt-screen mode.
        let mut warnings = Vec::new();
        if let Err(e) = try_save(path, &cfg) {
            warnings.push(LoadWarning(format!("Failed to write migrated config: {e}")));
        }
        return Ok((cfg, warnings));
    }

    let contents = fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&contents).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), e),
        )
    })?;

    let mut warnings = Vec::new();
    if Theme::try_from_label(&cfg.theme).is_none() {
        warnings.push(LoadWarning(format!(
            "Unknown theme '{}', using Default",
            cfg.theme
        )));
    }

    Ok((cfg, warnings))
}

/// Atomically persist `config` to `path`.
///
/// Writes to a sibling `<name>.tmp` file then `fs::rename`s it into
/// place. On POSIX `rename` is atomic; on Windows it's atomic for the
/// same-volume case that applies here. A crash mid-write leaves the
/// previous good file intact rather than a zero-byte stub.
///
/// If anything fails after the temp file is created, we try to remove
/// it so successive runs don't accumulate stale `*.tmp` siblings.
pub fn try_save(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Build "<original-name>.tmp" sitting next to the target so the
    // rename below stays on the same filesystem (a cross-device rename
    // would fall back to copy+unlink and lose atomicity).
    let tmp = match path.file_name() {
        Some(name) => {
            let mut tmp_name = name.to_os_string();
            tmp_name.push(".tmp");
            path.with_file_name(tmp_name)
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("config path has no file name: {}", path.display()),
            ));
        }
    };

    if let Err(e) = fs::write(&tmp, &contents) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    Ok(())
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
