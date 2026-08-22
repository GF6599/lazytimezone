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
//! and `favorites`) are imported automatically, and removed once the
//! unified file is safely written.
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
use std::io::Write;
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
    config_dir_from(
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// `std::env::var` reports a set-but-empty variable as `Ok("")`, which
/// would resolve to a relative `lazytimezone/config.toml` in whatever
/// directory the app was launched from. Hence the emptiness filter.
fn config_dir_from(xdg_config_home: Option<String>, home: Option<String>) -> Option<PathBuf> {
    let non_empty = |value: String| (!value.is_empty()).then_some(value);
    xdg_config_home
        .and_then(non_empty)
        .map(PathBuf::from)
        .or_else(|| {
            home.and_then(non_empty)
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
/// runs so first-launch upgrades behave the same as before. `path` is
/// the only place migration looks, including for the legacy files.
pub fn try_load(path: &Path) -> io::Result<(Config, Vec<LoadWarning>)> {
    if !path.exists() {
        let app_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let (cfg, legacy_files) = migrate_legacy(app_dir);
        // Migration is a one-shot upgrade — persist the result so legacy
        // files don't get re-read on the next launch. Errors here are
        // surfaced as a LoadWarning so the TUI status bar can display
        // them; stderr is invisible in alt-screen mode.
        let mut warnings = Vec::new();
        match try_save(path, &cfg) {
            Ok(()) => {
                for legacy in legacy_files {
                    let _ = fs::remove_file(legacy);
                }
            }
            // Until this write succeeds the legacy files are the user's
            // only remaining copy of their settings.
            Err(e) => {
                warnings.push(LoadWarning(format!("Failed to write migrated config: {e}")));
            }
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
/// Writes to a sibling `<name>.tmp` file, flushes it, then `fs::rename`s
/// it into place. On POSIX `rename` is atomic; on Windows it's atomic
/// for the same-volume case that applies here. A crash mid-write leaves
/// the previous good file intact rather than a zero-byte stub.
///
/// The flush is what makes that last sentence true: without it the
/// rename can reach the disk ahead of the bytes it points at.
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

    if let Err(e) = write_and_sync(&tmp, contents.as_bytes()) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    Ok(())
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Imports legacy plain-text `theme` and `favorites` files from `app_dir`.
///
/// The returned paths are the ones that existed and were read. Removing
/// them belongs to the caller, once the migrated config is on disk.
fn migrate_legacy(app_dir: &Path) -> (Config, Vec<PathBuf>) {
    let theme_path = app_dir.join("theme");
    let favorites_path = app_dir.join("favorites");
    let mut read_files = Vec::new();

    let theme = match fs::read_to_string(&theme_path) {
        Ok(contents) => {
            read_files.push(theme_path);
            Some(contents.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(default_theme)
        }
        Err(_) => default_theme(),
    };

    let favorites = match fs::read_to_string(&favorites_path) {
        Ok(contents) => {
            read_files.push(favorites_path);
            contents
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(_) => Vec::new(),
    };

    (Config { theme, favorites }, read_files)
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    /// An isolated config tree under the OS temp directory, removed on drop.
    ///
    /// Every test gets its own so the suite stays order-independent under
    /// the default multi-threaded runner.
    struct TempConfig {
        root: PathBuf,
    }

    impl TempConfig {
        fn new() -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "lazytimezone-test-{}-{}",
                std::process::id(),
                id
            ));
            let this = Self { root };
            fs::create_dir_all(this.app_dir()).unwrap();
            this
        }

        fn app_dir(&self) -> PathBuf {
            self.root.join("lazytimezone")
        }

        fn config_path(&self) -> PathBuf {
            self.app_dir().join("config.toml")
        }

        fn write_legacy(&self, theme: &str, favorites: &str) {
            fs::write(self.app_dir().join("theme"), theme).unwrap();
            fs::write(self.app_dir().join("favorites"), favorites).unwrap();
        }

        fn legacy_files_exist(&self) -> bool {
            self.app_dir().join("theme").exists() || self.app_dir().join("favorites").exists()
        }

        /// Occupies the atomic-write temp path with a non-empty directory
        /// so `try_save` cannot write it. Stands in for a read-only or
        /// full config directory.
        fn block_saving(&self) {
            let blocker = self.app_dir().join("config.toml.tmp");
            fs::create_dir_all(&blocker).unwrap();
            fs::write(blocker.join("occupied"), b"x").unwrap();
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn migration_reads_legacy_files_from_the_directory_it_was_given() {
        let tmp = TempConfig::new();
        tmp.write_legacy("Nord\n", "Asia/Tokyo\nEurope/London\n");

        let (cfg, _) = try_load(&tmp.config_path()).unwrap();

        assert_eq!(cfg.theme, "Nord");
        assert_eq!(cfg.favorites, vec!["Asia/Tokyo", "Europe/London"]);
    }

    #[test]
    fn legacy_files_survive_a_failed_save() {
        let tmp = TempConfig::new();
        tmp.write_legacy("Nord\n", "Asia/Tokyo\n");
        tmp.block_saving();

        let (cfg, warnings) = try_load(&tmp.config_path()).unwrap();

        assert_eq!(cfg.theme, "Nord", "the legacy theme should still be read");
        assert!(!warnings.is_empty(), "the failed save should be reported");
        assert!(
            tmp.legacy_files_exist(),
            "legacy files must not be deleted when the migrated config was never written"
        );
    }

    #[test]
    fn legacy_files_are_removed_once_the_save_succeeds() {
        let tmp = TempConfig::new();
        tmp.write_legacy("Gruvbox\n", "Asia/Tokyo\n");

        let (_, warnings) = try_load(&tmp.config_path()).unwrap();

        assert!(warnings.is_empty(), "the save was expected to succeed");
        assert!(
            tmp.config_path().exists(),
            "the migrated config should exist"
        );
        assert!(
            !tmp.legacy_files_exist(),
            "legacy files should be cleaned up after a successful migration"
        );
    }

    #[test]
    fn a_malformed_file_is_reported_and_left_untouched() {
        let tmp = TempConfig::new();
        let original = "theme = \"Nord\"\nfavorites = [ oops";
        fs::write(tmp.config_path(), original).unwrap();

        let err = try_load(&tmp.config_path()).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read_to_string(tmp.config_path()).unwrap(),
            original,
            "a parse error must never rewrite the user's file"
        );
    }

    #[test]
    fn an_empty_xdg_config_home_falls_back_to_home() {
        let dir = config_dir_from(Some(String::new()), Some("/home/ada".to_string()));

        assert_eq!(dir, Some(PathBuf::from("/home/ada/.config")));
    }

    #[test]
    fn a_populated_xdg_config_home_wins_over_home() {
        let dir = config_dir_from(Some("/xdg".to_string()), Some("/home/ada".to_string()));

        assert_eq!(dir, Some(PathBuf::from("/xdg")));
    }

    #[test]
    fn two_empty_environment_values_resolve_to_no_directory() {
        let dir = config_dir_from(Some(String::new()), Some(String::new()));

        assert_eq!(dir, None);
    }

    #[test]
    fn an_unknown_theme_loads_with_a_warning_rather_than_failing() {
        let tmp = TempConfig::new();
        fs::write(tmp.config_path(), "theme = \"Nonesuch\"\n").unwrap();

        let (cfg, warnings) = try_load(&tmp.config_path()).unwrap();

        assert_eq!(cfg.theme, "Nonesuch");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].0.contains("Nonesuch"));
    }
}
