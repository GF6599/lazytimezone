//! # Unified TOML configuration: load and save.
//!
//! All persistent state lives in `~/.config/lazytimezone/config.toml`:
//!
//! ```toml
//! theme = "Nord"
//! wall_view = "map"
//!
//! [[favorites]]
//! city = "Boston"
//! admin1 = "Massachusetts"
//! cc = "US"
//! ```
//!
//! A favorite from the pre-city format, a bare IANA zone string such
//! as `"Asia/Tokyo"`, still parses ([`SavedCity::Zone`]) and is
//! resolved to the zone's major city at load.
//!
//! ## API shape
//!
//! - [`try_load`] / [`try_save`] take an explicit path and surface
//!   [`io::Result`] so a caller can choose to abort, warn, or fall back
//!   to defaults without auto-clobbering a malformed user file.
//! - [`default_path`] gives the canonical path, used when no `--config`
//!   flag names another.

use crate::theme::Theme;
use crate::timezone::TimezoneEntry;
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
    pub favorites: Vec<SavedCity>,
    /// The label of the view under the clock. The app owns the labels,
    /// and a file without the key opens on the panels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_view: Option<String>,
}

/// A city as the config file saves it.
///
/// `Zone` is the pre-city format of a favorite. It still deserializes
/// so an old config keeps its favorites, and saving writes the `City`
/// form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedCity {
    City {
        city: String,
        admin1: String,
        cc: String,
    },
    Zone(String),
}

impl SavedCity {
    /// The saved form of a catalogue row.
    pub fn of(entry: &TimezoneEntry) -> Self {
        SavedCity::City {
            city: entry.city.to_string(),
            admin1: entry.admin1.to_string(),
            cc: entry.cc.to_string(),
        }
    }

    /// The catalogue row this names. A `Zone` names the zone's most
    /// populous city, which is its first row in the catalogue.
    pub fn find_in(&self, entries: &[TimezoneEntry]) -> Option<usize> {
        match self {
            SavedCity::City { city, admin1, cc } => entries
                .iter()
                .position(|e| e.city == city && e.admin1 == admin1 && e.cc == cc),
            SavedCity::Zone(zone) => zone
                .parse::<chrono_tz::Tz>()
                .ok()
                .and_then(|tz| entries.iter().position(|e| e.tz == tz)),
        }
    }

    /// The name a warning about this city shows.
    pub fn name(&self) -> &str {
        match self {
            SavedCity::City { city, .. } => city,
            SavedCity::Zone(zone) => zone,
        }
    }
}

fn default_theme() -> String {
    "Default".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: default_theme(),
            favorites: vec![],
            wall_view: None,
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

/// Canonical on-disk config path, or `None` if neither `$XDG_CONFIG_HOME`
/// nor `$HOME` is set.
pub fn default_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("lazytimezone").join("config.toml"))
}

/// Path-aware loader that distinguishes the three states a caller cares
/// about: file missing (defaults, no warnings), file present and parsed
/// (config + any soft warnings), file present but unreadable / malformed
/// (error).
///
/// On a parse error the caller must NOT auto-save, because that would clobber
/// the user's broken file before they have a chance to fix it.
pub fn try_load(path: &Path) -> io::Result<(Config, Vec<LoadWarning>)> {
    if !path.exists() {
        return Ok((Config::default(), Vec::new()));
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
/// it into place. When `path` is a symlink, the sibling and the rename
/// are at the file it names, so the link stays. On POSIX `rename` is atomic; on Windows it's atomic
/// for the same-volume case that applies here. A crash mid-write leaves
/// the previous good file intact rather than a zero-byte stub.
///
/// The flush is what makes that last sentence true: without it the
/// rename can reach the disk ahead of the bytes it points at.
///
/// If anything fails after the temp file is created, we try to remove
/// it so successive runs don't accumulate stale `*.tmp` siblings.
pub fn try_save(path: &Path, config: &Config) -> io::Result<()> {
    let path = &save_target(path);
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

/// The file a save replaces: `path`, or the file its symlink names, so
/// the rename lands on the target and the link stays a link.
fn save_target(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    // A link to a file that does not exist yet has no canonical form.
    match fs::read_link(path) {
        Ok(target) => match path.parent() {
            Some(dir) => dir.join(target),
            None => target,
        },
        Err(_) => path.to_path_buf(),
    }
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design. See src/app.rs for the rationale
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
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn city_favorites_round_trip_through_toml() {
        let tmp = TempConfig::new();
        let cfg = Config {
            theme: "Nord".to_string(),
            favorites: vec![
                SavedCity::City {
                    city: "Boston".to_string(),
                    admin1: "Massachusetts".to_string(),
                    cc: "US".to_string(),
                },
                SavedCity::Zone("Asia/Tokyo".to_string()),
            ],
            wall_view: Some("map".to_string()),
        };

        try_save(&tmp.config_path(), &cfg).unwrap();
        let (loaded, warnings) = try_load(&tmp.config_path()).unwrap();

        assert!(warnings.is_empty());
        assert_eq!(loaded.favorites, cfg.favorites);
        assert_eq!(loaded.wall_view, cfg.wall_view);
    }

    /// A config path that does not exist yet loads the defaults and
    /// leaves the files beside it alone.
    ///
    /// Given: files named `theme` and `favorites` in the folder of a
    /// config path that does not exist.
    /// When: the config loads.
    /// Then: it holds the defaults with no warning, and both files are
    /// still there with their content.
    ///
    /// Why it matters: `--config` can name a file in any folder, such as
    /// a project folder, and a load must never read or delete files the
    /// app did not write.
    #[test]
    fn a_missing_config_loads_defaults_and_leaves_its_folder_alone() {
        // Given:
        let tmp = TempConfig::new();
        let theme = tmp.app_dir().join("theme");
        let favorites = tmp.app_dir().join("favorites");
        fs::write(&theme, "Nord\n").unwrap();
        fs::write(&favorites, "Asia/Tokyo\n").unwrap();

        // When:
        let (cfg, warnings) = try_load(&tmp.config_path()).unwrap();

        // Then:
        assert_eq!(cfg.theme, default_theme());
        assert!(cfg.favorites.is_empty());
        assert!(warnings.is_empty());
        assert_eq!(fs::read_to_string(&theme).unwrap(), "Nord\n");
        assert_eq!(fs::read_to_string(&favorites).unwrap(), "Asia/Tokyo\n");
    }

    /// A save through a symlink writes the file the link names and
    /// keeps the link.
    ///
    /// Given: the config path is a symlink to a file in another folder,
    /// and a second path is a symlink to a file that does not exist yet.
    /// When: a config is saved to each path.
    /// Then: both paths are still symlinks, and each target holds the
    /// saved config.
    ///
    /// Why it matters: dotfile managers such as stow link the config
    /// into place, and a save that replaces the link splits it from the
    /// file the user keeps.
    #[cfg(unix)]
    #[test]
    fn a_save_through_a_symlink_keeps_the_link() {
        // Given:
        let tmp = TempConfig::new();
        let dotfiles = tmp.root.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        let existing = dotfiles.join("config.toml");
        fs::write(&existing, "theme = \"Nord\"\n").unwrap();
        std::os::unix::fs::symlink(&existing, tmp.config_path()).unwrap();
        let dangling = tmp.app_dir().join("second.toml");
        std::os::unix::fs::symlink("../dotfiles/second.toml", &dangling).unwrap();
        let cfg = Config {
            theme: "Gruvbox".to_string(),
            ..Config::default()
        };

        for (link, target) in [
            (tmp.config_path(), existing),
            (dangling, dotfiles.join("second.toml")),
        ] {
            // When:
            try_save(&link, &cfg).unwrap();

            // Then:
            let kind = fs::symlink_metadata(&link).unwrap().file_type();
            assert!(kind.is_symlink(), "{} is no longer a link", link.display());
            assert_eq!(try_load(&target).unwrap().0.theme, "Gruvbox");
        }
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
