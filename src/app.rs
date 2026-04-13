//! # Core application state and business logic.
//!
//! [`App`] is the single source of truth for the entire application.
//! The event loop in `main` mutates it via public methods, and the UI
//! layer reads it to render each frame.
//!
//! ## State groups
//!
//! | Group | Fields | Persisted? |
//! |-------|--------|-----------|
//! | Navigation | `selected_row`, `filtered_indices` | No |
//! | Selection | `selected_timezone`, `selected_city_name` | No |
//! | Search | `input_mode`, `search_query`, `cursor_position` | No |
//! | Theme | `theme` | Yes (`~/.config/lazytimezone/config.toml`) |
//! | Favorites | `favorites`, `show_favorites_only` | Yes (`~/.config/lazytimezone/config.toml`) |
//! | Feedback | `copied_flash` | No |

use std::io::Write;
use std::time::Instant;

use chrono::Utc;
use chrono::offset::Offset;

use crate::config;
use crate::theme::Theme;
use crate::timezone::{TimezoneEntry, all_timezones};
use crate::ui::format_utc_offset;

/// Whether the app is accepting navigation keys or search text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

/// Central application state — owned exclusively by the event loop.
///
/// ## Pattern: Index-Based Filtering
///
/// Rather than cloning or re-sorting the timezone list on every
/// keystroke, `filtered_indices` holds indices into the immutable
/// `timezones` vec. Searching and favouriting only rearrange these
/// indices, keeping the underlying data allocation-free.
pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub search_query: String,
    /// Byte offset into `search_query` (not char index), because
    /// `String::insert` and `String::remove` operate on byte positions.
    pub cursor_position: usize,

    /// Full, immutable timezone catalogue loaded once at startup.
    pub timezones: Vec<TimezoneEntry>,
    /// Indices into `timezones` after applying search + favorites filter.
    /// This is the "view" that the table renders.
    pub filtered_indices: Vec<usize>,
    /// Currently highlighted row in `filtered_indices`.
    pub selected_row: usize,

    /// The timezone whose time is shown in the big clock and used as
    /// the baseline for the "Diff" column.
    pub selected_timezone: chrono_tz::Tz,
    pub selected_city_name: String,

    pub theme: Theme,
    /// Ordered list of IANA timezone names (e.g. `"Asia/Kolkata"`).
    /// Order determines display priority and side-clock slots.
    pub favorites: Vec<String>,
    pub show_favorites_only: bool,
    /// Set to `Some(Instant::now())` after a clipboard copy; the UI
    /// shows "Copied!" for 2 seconds then clears it.
    pub copied_flash: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        let timezones = all_timezones();
        let cfg = config::load();
        let theme = Theme::from_label(&cfg.theme);
        let favorites = cfg.favorites;
        let mut filtered_indices: Vec<usize> = (0..timezones.len()).collect();
        Self::sort_indices(&mut filtered_indices, &timezones, &favorites);
        Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            cursor_position: 0,
            filtered_indices,
            selected_row: 0,
            selected_timezone: chrono_tz::Tz::UTC,
            selected_city_name: "UTC".to_string(),
            timezones,
            theme,
            favorites,
            show_favorites_only: false,
            copied_flash: None,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_row < self.filtered_indices.len() - 1
        {
            self.selected_row += 1;
        }
    }

    pub fn page_up(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_row = (self.selected_row + 10).min(self.filtered_indices.len() - 1);
        }
    }

    pub fn home(&mut self) {
        self.selected_row = 0;
    }

    pub fn end(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_row = self.filtered_indices.len() - 1;
        }
    }

    pub fn select_timezone(&mut self) {
        if let Some(&idx) = self.filtered_indices.get(self.selected_row) {
            let entry = &self.timezones[idx];
            self.selected_timezone = entry.tz;
            self.selected_city_name = entry.city.to_string();
        }
    }

    pub fn enter_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.cursor_position = 0;
    }

    pub fn exit_search(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn search_input(&mut self, c: char) {
        self.search_query.insert(self.cursor_position, c);
        self.cursor_position += c.len_utf8();
        self.apply_filter();
    }

    pub fn search_backspace(&mut self) {
        if self.cursor_position > 0 {
            let prev = self.search_query[..self.cursor_position]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_position -= prev;
            self.search_query.remove(self.cursor_position);
            self.apply_filter();
        }
    }

    pub fn clear_search_input(&mut self) {
        self.search_query.clear();
        self.cursor_position = 0;
        self.apply_filter();
    }

    /// Rebuilds `filtered_indices` from the current search query and
    /// favorites filter.
    ///
    /// ## Scoring algorithm
    ///
    /// When a search query is active, each timezone is scored using
    /// additive term matching:
    ///
    /// | Match type | Points |
    /// |-----------|--------|
    /// | Exact city name | 100 |
    /// | City starts with term | 75 |
    /// | City contains term | 50 |
    /// | Country contains term | 30 |
    /// | Region contains term | 25 |
    /// | Is a favourite | +10 (bonus) |
    ///
    /// All terms must match (AND logic). The bare term `"utc"` is
    /// normalised to `"utc+0"` so it matches the offset string
    /// rather than failing silently.
    pub fn apply_filter(&mut self) {
        let base_indices: Vec<usize> = if self.show_favorites_only {
            (0..self.timezones.len())
                .filter(|&i| {
                    self.favorites
                        .iter()
                        .any(|n| n == &self.timezones[i].tz.to_string())
                })
                .collect()
        } else {
            (0..self.timezones.len()).collect()
        };

        if self.search_query.is_empty() {
            self.filtered_indices = base_indices;
            Self::sort_indices(&mut self.filtered_indices, &self.timezones, &self.favorites);
        } else {
            let now = Utc::now();
            let original_terms: Vec<String> = self
                .search_query
                .to_lowercase()
                .split_whitespace()
                .map(String::from)
                .collect();
            let filter_terms: Vec<String> = original_terms
                .iter()
                .map(|t| {
                    if t == "utc" {
                        "utc+0".to_string()
                    } else {
                        t.clone()
                    }
                })
                .collect();
            let mut scored: Vec<(usize, u32)> = base_indices
                .into_iter()
                .filter_map(|i| {
                    let entry = &self.timezones[i];
                    let city = entry.city.to_lowercase();
                    let country = entry.country.to_lowercase();
                    let region = entry.region.to_lowercase();
                    let aliases_lower: Vec<String> =
                        entry.aliases.iter().map(|a| a.to_lowercase()).collect();
                    let offset_secs = now
                        .with_timezone(&entry.tz)
                        .offset()
                        .fix()
                        .local_minus_utc();
                    let offset_str = format_utc_offset(offset_secs).to_lowercase();
                    let aliases_str = aliases_lower.join(" ");
                    let haystack = format!(
                        "{} {} {} {} {}",
                        city, country, region, offset_str, aliases_str
                    );
                    let all_match = filter_terms.iter().all(|t| haystack.contains(t.as_str()));
                    if !all_match {
                        return None;
                    }
                    let mut score: u32 = 0;
                    for term in &original_terms {
                        if city == *term {
                            score += 100;
                        } else if city.starts_with(term.as_str()) {
                            score += 75;
                        } else if city.contains(term.as_str()) {
                            score += 50;
                        } else if aliases_lower.iter().any(|a| a == term) {
                            score += 45;
                        } else if aliases_lower.iter().any(|a| a.contains(term.as_str())) {
                            score += 40;
                        } else if country.contains(term.as_str()) {
                            score += 30;
                        } else if region.contains(term.as_str()) {
                            score += 25;
                        }
                    }
                    if self.favorites.iter().any(|n| n == &entry.tz.to_string()) {
                        score += 10;
                    }
                    Some((i, score))
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| self.timezones[a.0].city.cmp(&self.timezones[b.0].city))
            });
            self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        }
        if self.filtered_indices.is_empty() {
            self.selected_row = 0;
        } else if self.selected_row >= self.filtered_indices.len() {
            self.selected_row = self.filtered_indices.len() - 1;
        }
    }

    pub fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        self.save_config();
    }

    pub fn toggle_favorites_filter(&mut self) {
        self.show_favorites_only = !self.show_favorites_only;
        self.apply_filter();
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(&idx) = self.filtered_indices.get(self.selected_row) {
            let tz_name = self.timezones[idx].tz.to_string();
            if let Some(pos) = self.favorites.iter().position(|n| n == &tz_name) {
                self.favorites.remove(pos);
            } else {
                self.favorites.push(tz_name);
            }
            self.save_config();
            self.apply_filter();
        }
    }

    pub fn move_favorite_up(&mut self) {
        if let Some(&idx) = self.filtered_indices.get(self.selected_row) {
            let tz_name = self.timezones[idx].tz.to_string();
            if let Some(pos) = self.favorites.iter().position(|n| n == &tz_name) {
                if pos > 0 {
                    self.favorites.swap(pos, pos - 1);
                    self.save_config();
                    self.apply_filter();
                    self.selected_row = self.selected_row.saturating_sub(1);
                }
            }
        }
    }

    pub fn move_favorite_down(&mut self) {
        if let Some(&idx) = self.filtered_indices.get(self.selected_row) {
            let tz_name = self.timezones[idx].tz.to_string();
            if let Some(pos) = self.favorites.iter().position(|n| n == &tz_name) {
                if pos + 1 < self.favorites.len() {
                    self.favorites.swap(pos, pos + 1);
                    self.save_config();
                    self.apply_filter();
                    if self.selected_row + 1 < self.filtered_indices.len() {
                        self.selected_row += 1;
                    }
                }
            }
        }
    }

    pub fn top_favorite_timezones(&self, count: usize) -> Vec<(chrono_tz::Tz, &str)> {
        self.favorites
            .iter()
            .take(count)
            .filter_map(|name| {
                self.timezones
                    .iter()
                    .find(|e| e.tz.to_string() == *name)
                    .map(|e| (e.tz, e.city))
            })
            .collect()
    }

    pub fn is_favorite(&self, tz_index: usize) -> bool {
        let tz_name = self.timezones[tz_index].tz.to_string();
        self.favorites.iter().any(|n| n == &tz_name)
    }

    /// Sorts indices so favourites appear first (in user-defined order),
    /// followed by non-favourites sorted alphabetically by city name.
    fn sort_indices(indices: &mut Vec<usize>, timezones: &[TimezoneEntry], favorites: &[String]) {
        indices.sort_by(|&a, &b| {
            let a_name = timezones[a].tz.to_string();
            let b_name = timezones[b].tz.to_string();
            let a_fav = favorites.iter().position(|n| n == &a_name);
            let b_fav = favorites.iter().position(|n| n == &b_name);
            match (a_fav, b_fav) {
                (Some(ai), Some(bi)) => ai.cmp(&bi),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => timezones[a].city.cmp(&timezones[b].city),
            }
        });
    }

    fn save_config(&self) {
        config::save(&config::Config {
            theme: self.theme.label().to_string(),
            favorites: self.favorites.clone(),
        });
    }

    /// Copies the selected timezone's current time to the system
    /// clipboard in compact ISO-ish format (`YYYYMMDDTHHMMTz`).
    ///
    /// Uses platform-specific clipboard tools:
    /// - macOS: `pbcopy`
    /// - Windows: `clip`
    /// - Linux: `wl-copy` (Wayland) with `xclip` fallback (X11)
    pub fn copy_time(&mut self) {
        let now = Utc::now().with_timezone(&self.selected_timezone);
        let formatted = now.format("%Y%m%dT%H%M%Z").to_string();
        for (cmd, args) in clipboard_commands() {
            if pipe_to_command(cmd, args, &formatted) {
                self.copied_flash = Some(Instant::now());
                return;
            }
        }
    }
}

/// Pipes `text` into a command's stdin and returns `true` if it succeeds.
fn pipe_to_command(cmd: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().is_ok_and(|s| s.success())
}

/// Returns the platform-appropriate clipboard command(s) to try, in priority order.
#[cfg(target_os = "macos")]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![("pbcopy", &[])]
}

#[cfg(target_os = "windows")]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![("clip", &[])]
}

#[cfg(target_os = "linux")]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"] as &[&str]),
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![]
}
