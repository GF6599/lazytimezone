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

use std::collections::HashMap;
use std::io::Write;
use std::time::Instant;

use chrono::Utc;
use chrono::offset::Offset;
use chrono_tz::Tz;

use crate::config;
use crate::theme::Theme;
use crate::timezone::{
    SupplementalSearchTerm, TimezoneEntry, all_timezones, country_search_aliases,
    supplemental_search_terms,
};

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
    /// Maps each catalogue `Tz` to its index in `timezones`. Built
    /// once at startup; never mutated.
    tz_to_index: HashMap<Tz, usize>,
    /// Pre-normalized search metadata for each timezone entry.
    search_index: Vec<TimezoneSearchData>,
    /// Indices into `timezones` after applying search + favorites filter.
    /// This is the "view" that the table renders.
    pub filtered_indices: Vec<usize>,
    /// City labels shown beside `filtered_indices`.
    ///
    /// This is usually the representative city for the timezone, but
    /// while searching it may be the matched alias (for example
    /// searching `boston` shows `Boston` instead of `New York`).
    pub filtered_display_names: Vec<&'static str>,
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
    /// Maps each favorite `Tz` to its position in `favorites` for
    /// O(1) membership checks and order-preserving sorts. Rebuilt
    /// whenever `favorites` changes.
    favorites_order: HashMap<Tz, usize>,
    /// Set to `Some(Instant::now())` after a clipboard copy; the UI
    /// shows "Copied!" for 2 seconds then clears it.
    pub copied_flash: Option<Instant>,
}

struct SearchText {
    raw: &'static str,
    normalized: String,
}

impl SearchText {
    fn new(raw: &'static str) -> Self {
        Self {
            raw,
            normalized: normalize_search_text(raw),
        }
    }
}

struct SearchKeyword {
    text: SearchText,
    display_in_results: bool,
}

impl SearchKeyword {
    fn new(term: SupplementalSearchTerm) -> Self {
        Self {
            text: SearchText::new(term.raw),
            display_in_results: term.display_in_results,
        }
    }
}

struct TimezoneSearchData {
    city: SearchText,
    country: String,
    region: String,
    timezone_words: String,
    aliases: Vec<SearchText>,
    country_aliases: Vec<String>,
    keywords: Vec<SearchKeyword>,
}

impl TimezoneSearchData {
    fn new(entry: &TimezoneEntry) -> Self {
        Self {
            city: SearchText::new(entry.city),
            country: normalize_search_text(entry.country),
            region: normalize_search_text(entry.region),
            timezone_words: normalize_search_text(&entry.tz.to_string()),
            aliases: entry
                .aliases
                .iter()
                .map(|alias| SearchText::new(alias))
                .filter(|alias| !alias.normalized.is_empty())
                .collect(),
            country_aliases: country_search_aliases(entry.country)
                .iter()
                .map(|alias| normalize_search_text(alias))
                .filter(|alias| !alias.is_empty())
                .collect(),
            keywords: supplemental_search_terms(entry)
                .iter()
                .copied()
                .map(SearchKeyword::new)
                .filter(|term| !term.text.normalized.is_empty())
                .collect(),
        }
    }
}

struct SearchQuery {
    normalized: String,
    terms: Vec<String>,
}

impl SearchQuery {
    fn new(query: &str) -> Self {
        let normalized = normalize_search_text(query);
        let mut terms: Vec<String> = normalized.split_whitespace().map(String::from).collect();
        if matches!(terms.as_slice(), [single] if matches!(single.as_str(), "utc" | "gmt")) {
            terms[0] = "utc+0".to_string();
        }
        Self { normalized, terms }
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_config(config::load())
    }

    fn with_config(cfg: config::Config) -> Self {
        let timezones = all_timezones();
        let tz_to_index = timezones
            .iter()
            .enumerate()
            .map(|(i, e)| (e.tz, i))
            .collect();
        let search_index = timezones.iter().map(TimezoneSearchData::new).collect();
        let theme = Theme::from_label(&cfg.theme);
        let favorites = cfg.favorites;
        let favorites_order = Self::build_favorites_order(&favorites);
        let mut filtered_indices: Vec<usize> = (0..timezones.len()).collect();
        Self::sort_indices(&mut filtered_indices, &timezones, &favorites_order);
        let filtered_display_names = filtered_indices
            .iter()
            .map(|&idx| timezones[idx].city)
            .collect();
        Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            cursor_position: 0,
            filtered_indices,
            filtered_display_names,
            selected_row: 0,
            selected_timezone: chrono_tz::Tz::UTC,
            selected_city_name: "UTC".to_string(),
            timezones,
            tz_to_index,
            search_index,
            theme,
            favorites,
            favorites_order,
            show_favorites_only: false,
            copied_flash: None,
        }
    }

    fn build_favorites_order(favorites: &[String]) -> HashMap<Tz, usize> {
        favorites
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.parse::<Tz>().ok().map(|tz| (tz, i)))
            .collect()
    }

    fn rebuild_favorites_order(&mut self) {
        self.favorites_order = Self::build_favorites_order(&self.favorites);
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
        self.selected_row = self.filtered_indices.len().saturating_sub(1);
    }

    pub fn select_timezone(&mut self) {
        if let Some((idx, display_name)) = self.current_result() {
            let entry = &self.timezones[idx];
            self.selected_timezone = entry.tz;
            self.selected_city_name = display_name.to_string();
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
        self.select_first_result();
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
            self.select_first_result();
        }
    }

    pub fn clear_search_input(&mut self) {
        self.search_query.clear();
        self.cursor_position = 0;
        self.apply_filter();
        self.select_first_result();
    }

    /// Rebuilds `filtered_indices` from the current search query and
    /// favorites filter.
    ///
    /// Search is case-insensitive and punctuation-insensitive.
    ///
    /// Queries are matched with AND logic across cities, aliases,
    /// countries, regions, IANA timezone identifiers, curated area
    /// keywords (for example state names and common timezone labels),
    /// and current UTC/GMT offset spellings such as `UTC-8`,
    /// `GMT-08:00`, and `+0530`. Exact and prefix matches rank above
    /// plain substrings.
    pub fn apply_filter(&mut self) {
        let base_indices = self.base_indices();

        if self.search_query.is_empty() {
            self.set_sorted_results(base_indices);
        } else {
            let query = SearchQuery::new(&self.search_query);
            if query.terms.is_empty() {
                self.set_sorted_results(base_indices);
                self.clamp_selected_row();
                return;
            }
            let now = Utc::now();
            let mut offset_cache: HashMap<i32, Vec<String>> = HashMap::new();
            let mut scored: Vec<(usize, &'static str, u32)> = base_indices
                .into_iter()
                .filter_map(|i| {
                    let entry = &self.timezones[i];
                    let offset_secs = now
                        .with_timezone(&entry.tz)
                        .offset()
                        .fix()
                        .local_minus_utc();
                    let offset_terms = offset_cache
                        .entry(offset_secs)
                        .or_insert_with(|| offset_search_terms(offset_secs));
                    let mut score =
                        score_phrase_match(&query, &self.search_index[i], offset_terms.as_slice());
                    for term in &query.terms {
                        let term_score =
                            score_search_term(term, &self.search_index[i], offset_terms.as_slice());
                        if term_score == 0 {
                            return None;
                        }
                        score += term_score;
                    }
                    if self.favorites_order.contains_key(&entry.tz) {
                        score += 10;
                    }
                    let display_name = best_display_name(entry, &self.search_index[i], &query);
                    Some((i, display_name, score))
                })
                .collect();
            scored.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| a.1.cmp(b.1))
                    .then_with(|| self.timezones[a.0].city.cmp(self.timezones[b.0].city))
            });
            self.set_results(
                scored
                    .into_iter()
                    .map(|(idx, display_name, _)| (idx, display_name)),
            );
        }
        self.clamp_selected_row();
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
        if let Some((idx, _)) = self.current_result() {
            let tz = self.timezones[idx].tz;
            if let Some(&pos) = self.favorites_order.get(&tz) {
                self.favorites.remove(pos);
            } else {
                self.favorites.push(tz.to_string());
            }
            self.rebuild_favorites_order();
            self.save_config();
            self.apply_filter();
        }
    }

    pub fn move_favorite_up(&mut self) {
        if let Some((idx, _)) = self.current_result()
            && let Some(&pos) = self.favorites_order.get(&self.timezones[idx].tz)
            && pos > 0
        {
            self.favorites.swap(pos, pos - 1);
            self.rebuild_favorites_order();
            self.save_config();
            self.apply_filter();
            self.selected_row = self.selected_row.saturating_sub(1);
        }
    }

    pub fn move_favorite_down(&mut self) {
        if let Some((idx, _)) = self.current_result()
            && let Some(&pos) = self.favorites_order.get(&self.timezones[idx].tz)
            && pos + 1 < self.favorites.len()
        {
            self.favorites.swap(pos, pos + 1);
            self.rebuild_favorites_order();
            self.save_config();
            self.apply_filter();
            if self.selected_row + 1 < self.filtered_indices.len() {
                self.selected_row += 1;
            }
        }
    }

    pub fn top_favorite_timezones(&self, count: usize) -> Vec<(chrono_tz::Tz, &str)> {
        self.favorites
            .iter()
            .take(count)
            .filter_map(|name| {
                let tz: Tz = name.parse().ok()?;
                let idx = *self.tz_to_index.get(&tz)?;
                Some((tz, self.timezones[idx].city))
            })
            .collect()
    }

    pub fn is_favorite(&self, tz_index: usize) -> bool {
        self.favorites_order
            .contains_key(&self.timezones[tz_index].tz)
    }

    fn current_result(&self) -> Option<(usize, &'static str)> {
        let tz_index = self.filtered_indices.get(self.selected_row).copied()?;
        let display_name = self
            .filtered_display_names
            .get(self.selected_row)
            .copied()
            .unwrap_or(self.timezones[tz_index].city);
        Some((tz_index, display_name))
    }

    fn base_indices(&self) -> Vec<usize> {
        if self.show_favorites_only {
            (0..self.timezones.len())
                .filter(|&i| self.favorites_order.contains_key(&self.timezones[i].tz))
                .collect()
        } else {
            (0..self.timezones.len()).collect()
        }
    }

    fn set_sorted_results(&mut self, mut indices: Vec<usize>) {
        Self::sort_indices(&mut indices, &self.timezones, &self.favorites_order);
        let results: Vec<_> = indices
            .into_iter()
            .map(|idx| (idx, self.timezones[idx].city))
            .collect();
        self.set_results(results);
    }

    fn set_results<I>(&mut self, results: I)
    where
        I: IntoIterator<Item = (usize, &'static str)>,
    {
        let (indices, display_names): (Vec<_>, Vec<_>) = results.into_iter().unzip();
        self.filtered_indices = indices;
        self.filtered_display_names = display_names;
    }

    fn clamp_selected_row(&mut self) {
        if self.filtered_indices.is_empty() {
            self.selected_row = 0;
        } else if self.selected_row >= self.filtered_indices.len() {
            self.selected_row = self.filtered_indices.len() - 1;
        }
    }

    fn select_first_result(&mut self) {
        self.selected_row = 0;
    }

    /// Sorts indices so favourites appear first (in user-defined order),
    /// followed by non-favourites sorted alphabetically by city name.
    fn sort_indices(
        indices: &mut [usize],
        timezones: &[TimezoneEntry],
        favorites_order: &HashMap<Tz, usize>,
    ) {
        indices.sort_by(|&a, &b| {
            let a_pos = favorites_order.get(&timezones[a].tz);
            let b_pos = favorites_order.get(&timezones[b].tz);
            match (a_pos, b_pos) {
                (Some(ai), Some(bi)) => ai.cmp(bi),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => timezones[a].city.cmp(timezones[b].city),
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

fn score_phrase_match(
    query: &SearchQuery,
    search: &TimezoneSearchData,
    offset_terms: &[String],
) -> u32 {
    if query.normalized.is_empty() {
        return 0;
    }

    let mut best = score_field(&search.city.normalized, &query.normalized, 220, 165, 105);
    best = best.max(best_score(
        search.aliases.iter().map(|alias| alias.normalized.as_str()),
        &query.normalized,
        180,
        135,
        85,
    ));
    best = best.max(best_score(
        search
            .keywords
            .iter()
            .map(|keyword| keyword.text.normalized.as_str()),
        &query.normalized,
        175,
        130,
        85,
    ));
    best = best.max(score_field(
        &search.timezone_words,
        &query.normalized,
        170,
        125,
        85,
    ));
    best = best.max(score_field(
        &search.country,
        &query.normalized,
        135,
        105,
        70,
    ));
    best = best.max(best_score(
        search.country_aliases.iter().map(String::as_str),
        &query.normalized,
        125,
        95,
        65,
    ));
    best = best.max(score_field(&search.region, &query.normalized, 110, 80, 55));
    best.max(best_score(
        offset_terms.iter().map(String::as_str),
        &query.normalized,
        120,
        95,
        70,
    ))
}

fn score_search_term(term: &str, search: &TimezoneSearchData, offset_terms: &[String]) -> u32 {
    let mut best = score_field(&search.city.normalized, term, 100, 75, 50);
    best = best.max(best_score(
        search.aliases.iter().map(|alias| alias.normalized.as_str()),
        term,
        90,
        68,
        45,
    ));
    best = best.max(best_score(
        search
            .keywords
            .iter()
            .map(|keyword| keyword.text.normalized.as_str()),
        term,
        85,
        64,
        42,
    ));
    best = best.max(score_field(&search.timezone_words, term, 85, 65, 45));
    best = best.max(score_field(&search.country, term, 60, 45, 30));
    best = best.max(best_score(
        search.country_aliases.iter().map(String::as_str),
        term,
        55,
        42,
        28,
    ));
    best = best.max(score_field(&search.region, term, 50, 38, 25));
    best.max(best_score(
        offset_terms.iter().map(String::as_str),
        term,
        70,
        55,
        40,
    ))
}

fn best_display_name(
    entry: &TimezoneEntry,
    search: &TimezoneSearchData,
    query: &SearchQuery,
) -> &'static str {
    let city_score = display_match_score(&search.city.normalized, query);
    let alias_match = best_search_text_match(search.aliases.iter(), query);
    let keyword_match = best_search_text_match(
        search
            .keywords
            .iter()
            .filter(|keyword| keyword.display_in_results)
            .map(|keyword| &keyword.text),
        query,
    );
    let best_non_city = alias_match
        .into_iter()
        .chain(keyword_match)
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));

    match best_non_city {
        Some((label, score)) if score > city_score && score > 0 => label,
        _ => entry.city,
    }
}

fn best_search_text_match<'a>(
    candidates: impl IntoIterator<Item = &'a SearchText>,
    query: &SearchQuery,
) -> Option<(&'static str, u32)> {
    candidates
        .into_iter()
        .map(|candidate| {
            (
                candidate.raw,
                display_match_score(&candidate.normalized, query),
            )
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)))
}

fn display_match_score(field: &str, query: &SearchQuery) -> u32 {
    let mut score = score_field(field, &query.normalized, 220, 165, 105);
    for term in &query.terms {
        score += score_field(field, term, 100, 75, 50);
    }
    score
}

fn best_score<'a>(
    fields: impl IntoIterator<Item = &'a str>,
    term: &str,
    exact: u32,
    prefix: u32,
    contains: u32,
) -> u32 {
    fields
        .into_iter()
        .map(|field| score_field(field, term, exact, prefix, contains))
        .max()
        .unwrap_or(0)
}

fn score_field(field: &str, term: &str, exact: u32, prefix: u32, contains: u32) -> u32 {
    if field.is_empty() || term.is_empty() {
        return 0;
    }
    if field == term {
        exact
    } else if field.starts_with(term)
        || field
            .split_whitespace()
            .any(|word| word_matches_term(word, term))
    {
        prefix
    } else if term.len() >= 3 && field.contains(term) {
        contains
    } else {
        0
    }
}

fn word_matches_term(word: &str, term: &str) -> bool {
    if word == term || word.starts_with(term) {
        return true;
    }
    let stripped = word.trim_start_matches(['+', '-']);
    stripped != word && (stripped == term || stripped.starts_with(term))
}

/// Formats a UTC offset in seconds to a human-readable string like
/// `UTC+5` or `UTC+5:30`. Used in both the table column and the
/// search scoring haystack.
pub fn format_utc_offset(total_secs: i32) -> String {
    let sign = if total_secs >= 0 { '+' } else { '-' };
    let abs = total_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    if mins == 0 {
        format!("UTC{}{}", sign, hours)
    } else {
        format!("UTC{}{}:{:02}", sign, hours, mins)
    }
}

fn offset_search_terms(total_secs: i32) -> Vec<String> {
    let sign = if total_secs >= 0 { '+' } else { '-' };
    let abs = total_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;

    let bare_canonical = if mins == 0 {
        format!("{sign}{hours}")
    } else {
        format!("{sign}{hours}:{mins:02}")
    };
    let bare_full = format!("{sign}{hours}:{mins:02}");
    let bare_padded = format!("{sign}{hours:02}:{mins:02}");
    let bare_compact = format!("{sign}{hours:02}{mins:02}");

    let variants = [
        bare_canonical,
        bare_full.clone(),
        bare_padded.clone(),
        bare_compact.clone(),
        format_utc_offset(total_secs),
        format!(
            "GMT{}",
            if mins == 0 {
                format!("{sign}{hours}")
            } else {
                format!("{sign}{hours}:{mins:02}")
            }
        ),
        format!("UTC{bare_full}"),
        format!("UTC{bare_padded}"),
        format!("UTC{bare_compact}"),
        format!("GMT{bare_full}"),
        format!("GMT{bare_padded}"),
        format!("GMT{bare_compact}"),
    ];

    let mut normalized = Vec::new();
    for variant in variants {
        push_unique(&mut normalized, normalize_search_text(&variant));
    }
    normalized
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_space = true;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        let next_is_digit = chars.peek().is_some_and(|next| next.is_ascii_digit());
        for lower in ch.to_lowercase() {
            match lower {
                'a'..='z' | '0'..='9' | '+' => {
                    normalized.push(lower);
                    last_was_space = false;
                }
                '-' if next_is_digit => {
                    normalized.push(lower);
                    last_was_space = false;
                }
                '\'' | '’' | '.' => {}
                _ => {
                    if !last_was_space {
                        normalized.push(' ');
                        last_was_space = true;
                    }
                }
            }
        }
    }

    while normalized.ends_with(' ') {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::with_config(config::Config::default())
    }

    fn apply_query(app: &mut App, query: &str) {
        app.search_query = query.to_string();
        app.cursor_position = query.len();
        app.apply_filter();
        app.selected_row = 0;
    }

    fn filtered_city_names(app: &App, count: usize) -> Vec<&'static str> {
        app.filtered_indices
            .iter()
            .take(count)
            .map(|&idx| app.timezones[idx].city)
            .collect()
    }

    fn filtered_display_names(app: &App, count: usize) -> Vec<&'static str> {
        app.filtered_display_names
            .iter()
            .take(count)
            .copied()
            .collect()
    }

    #[test]
    fn search_matches_iana_timezone_ids() {
        let mut app = test_app();

        apply_query(&mut app, "america/new_york");

        assert_eq!(app.timezones[app.filtered_indices[0]].city, "New York");
    }

    #[test]
    fn search_supports_country_aliases_and_punctuation_insensitive_queries() {
        let mut app = test_app();

        apply_query(&mut app, "united states");
        let us_matches = filtered_city_names(&app, 12);
        assert!(us_matches.contains(&"New York"));

        apply_query(&mut app, "st johns");
        let st_johns_matches = filtered_city_names(&app, 6);
        assert!(st_johns_matches.contains(&"St. John's"));
    }

    #[test]
    fn search_supports_state_and_timezone_family_terms() {
        let mut app = test_app();

        apply_query(&mut app, "texas");
        assert_eq!(app.timezones[app.filtered_indices[0]].city, "Chicago");
        assert_eq!(filtered_display_names(&app, 1), vec!["Texas"]);

        app.select_timezone();
        assert_eq!(app.selected_city_name, "Texas");

        apply_query(&mut app, "eastern time");
        assert_eq!(app.timezones[app.filtered_indices[0]].city, "New York");
        assert_eq!(filtered_display_names(&app, 1), vec!["New York"]);
    }

    #[test]
    fn search_displays_the_matching_alias_city() {
        let mut app = test_app();

        apply_query(&mut app, "boston");

        assert_eq!(app.timezones[app.filtered_indices[0]].city, "New York");
        assert_eq!(filtered_display_names(&app, 1), vec!["Boston"]);

        app.select_timezone();
        assert_eq!(app.selected_city_name, "Boston");
    }

    #[test]
    fn search_supports_gmt_and_compact_offset_formats() {
        let mut app = test_app();

        apply_query(&mut app, "gmt-10:00");
        let pacific_matches = filtered_city_names(&app, 6);
        assert!(pacific_matches.contains(&"Honolulu"));

        apply_query(&mut app, "+0530");
        let offset_matches = filtered_city_names(&app, 6);
        assert!(offset_matches.contains(&"Mumbai"));
        assert!(offset_matches.contains(&"Colombo"));
    }

    #[test]
    fn typing_a_new_query_moves_selection_to_the_top_result() {
        let mut app = test_app();

        app.selected_row = 25;
        app.search_input('l');

        assert_eq!(app.selected_row, 0);
    }

    // ── Navigation ──────────────────────────────────────────────────

    #[test]
    fn move_up_at_top_stays_at_zero() {
        let mut app = test_app();
        app.selected_row = 0;
        app.move_up();
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn move_down_at_bottom_stays_at_last() {
        let mut app = test_app();
        let last = app.filtered_indices.len() - 1;
        app.selected_row = last;
        app.move_down();
        assert_eq!(app.selected_row, last);
    }

    #[test]
    fn page_up_saturates_at_zero() {
        let mut app = test_app();
        app.selected_row = 3;
        app.page_up();
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn page_down_clamps_to_last() {
        let mut app = test_app();
        let last = app.filtered_indices.len() - 1;
        app.selected_row = last - 2;
        app.page_down();
        assert_eq!(app.selected_row, last);
    }

    #[test]
    fn home_and_end() {
        let mut app = test_app();
        app.selected_row = 50;
        app.home();
        assert_eq!(app.selected_row, 0);

        app.end();
        assert_eq!(app.selected_row, app.filtered_indices.len() - 1);
    }

    #[test]
    fn end_on_empty_list_is_safe() {
        let mut app = test_app();
        // Force an empty filter result
        apply_query(&mut app, "zzzzzznotaquery");
        assert!(app.filtered_indices.is_empty());

        app.end();
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn navigation_on_empty_list_is_safe() {
        let mut app = test_app();
        apply_query(&mut app, "zzzzzznotaquery");
        assert!(app.filtered_indices.is_empty());

        app.move_up();
        app.move_down();
        app.page_up();
        app.page_down();
        app.home();
        app.end();
        assert_eq!(app.selected_row, 0);
    }

    // ── Favorites ───────────────────────────────────────────────────

    #[test]
    fn toggle_favorite_adds_and_removes() {
        let mut app = test_app();
        app.selected_row = 0;
        app.select_timezone();
        let tz_name = app.selected_timezone.to_string();

        assert!(!app.favorites.contains(&tz_name));
        app.toggle_favorite();
        assert!(app.favorites.contains(&tz_name));
        assert!(app.favorites_order.contains_key(&app.selected_timezone));

        app.toggle_favorite();
        assert!(!app.favorites.contains(&tz_name));
        assert!(!app.favorites_order.contains_key(&app.selected_timezone));
    }

    #[test]
    fn favorites_filter_shows_only_favorites() {
        let mut app = test_app();
        let total = app.filtered_indices.len();

        // Add one favorite
        app.selected_row = 0;
        app.select_timezone();
        app.toggle_favorite();
        assert_eq!(app.filtered_indices.len(), total);

        // Toggle favorites-only mode
        app.toggle_favorites_filter();
        assert_eq!(app.filtered_indices.len(), 1);

        // Toggle back
        app.toggle_favorites_filter();
        assert_eq!(app.filtered_indices.len(), total);
    }

    #[test]
    fn favorites_appear_first_in_sort_order() {
        let mut app = test_app();
        // Navigate to a city that wouldn't normally be first alphabetically
        apply_query(&mut app, "tokyo");
        app.select_timezone();
        let tokyo_idx = app.filtered_indices[0];

        // Clear search and toggle favorite
        app.enter_search();
        app.exit_search();
        app.apply_filter();

        // Find Tokyo in the unfiltered list — shouldn't be first
        let pos_before = app
            .filtered_indices
            .iter()
            .position(|&i| i == tokyo_idx)
            .unwrap();
        assert!(pos_before > 0);

        // Favorite it
        app.selected_row = pos_before;
        app.toggle_favorite();

        // Now Tokyo should be first
        assert_eq!(app.filtered_indices[0], tokyo_idx);
    }

    // ── Theme ───────────────────────────────────────────────────────

    #[test]
    fn cycle_theme_wraps_around() {
        let mut app = test_app();
        assert_eq!(app.theme, Theme::Default);

        let themes = [
            Theme::Dracula,
            Theme::Solarized,
            Theme::Nord,
            Theme::Monokai,
            Theme::Gruvbox,
            Theme::Default,
        ];
        for expected in themes {
            app.theme = app.theme.next();
            assert_eq!(app.theme, expected);
        }
    }

    #[test]
    fn theme_from_label_round_trips() {
        for theme in [
            Theme::Default,
            Theme::Dracula,
            Theme::Solarized,
            Theme::Nord,
            Theme::Monokai,
            Theme::Gruvbox,
        ] {
            assert_eq!(Theme::from_label(theme.label()), theme);
        }
    }

    #[test]
    fn theme_from_label_unknown_defaults() {
        assert_eq!(Theme::from_label("NonExistent"), Theme::Default);
        assert_eq!(Theme::from_label(""), Theme::Default);
    }

    // ── Select timezone ─────────────────────────────────────────────

    #[test]
    fn select_timezone_updates_state() {
        let mut app = test_app();
        assert_eq!(app.selected_timezone, chrono_tz::Tz::UTC);

        apply_query(&mut app, "tokyo");
        app.select_timezone();
        assert_eq!(app.selected_timezone, chrono_tz::Tz::Asia__Tokyo);
        assert_eq!(app.selected_city_name, "Tokyo");
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
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("pbcopy", &[])]
}

#[cfg(target_os = "windows")]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("clip", &[])]
}

#[cfg(target_os = "linux")]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[]
}
