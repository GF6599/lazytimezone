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
//! | Selection | `selection` (`tz` + `city_name`) | No |
//! | Search | `input_mode`, `search_query`, `cursor_position` | No |
//! | Theme | `theme` | Yes (`~/.config/lazytimezone/config.toml`) |
//! | Favorites | `favorites`, `show_favorites_only` | Yes (`~/.config/lazytimezone/config.toml`) |
//! | Feedback | `copy_flash`, `startup_messages` | No |

use std::collections::HashMap;
use std::time::Instant;

use chrono::Utc;
use chrono_tz::Tz;

use crate::clipboard;
use crate::config;
use crate::search::SearchIndex;
use crate::theme::Theme;
use crate::timezone::{self, TimezoneEntry};

/// Immutable, derived-index-bearing timezone catalogue.
///
/// Bundles the full [`TimezoneEntry`] list together with the two indexes
/// derived from it — the `Tz`-to-position map and the search index — so
/// they can't drift out of sync. The struct exposes no mutators; the
/// catalogue is constructed once at startup and read thereafter.
pub(crate) struct Catalogue {
    entries: Vec<TimezoneEntry>,
    by_tz: HashMap<Tz, usize>,
    search_index: SearchIndex,
}

impl Catalogue {
    pub(crate) fn new() -> Self {
        let entries = timezone::all_timezones();
        let by_tz = entries.iter().enumerate().map(|(i, e)| (e.tz, i)).collect();
        let search_index = SearchIndex::build(&entries);
        Self {
            entries,
            by_tz,
            search_index,
        }
    }

    pub(crate) fn entries(&self) -> &[TimezoneEntry] {
        &self.entries
    }

    pub(crate) fn by_tz(&self, tz: Tz) -> Option<usize> {
        self.by_tz.get(&tz).copied()
    }

    pub(crate) fn search_index(&self) -> &SearchIndex {
        &self.search_index
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn get(&self, idx: usize) -> Option<&TimezoneEntry> {
        self.entries.get(idx)
    }
}

impl Default for Catalogue {
    fn default() -> Self {
        Self::new()
    }
}

/// Ordered list of favourite timezones with O(1) membership lookup.
///
/// Internally stores parsed [`Tz`] values (not strings) so search,
/// sort, and rendering paths never re-parse on the hot path. Strings
/// only appear at the on-disk boundary via
/// [`Favorites::from_strings`] / [`Favorites::to_strings`].
///
/// The `position` map is always a correct projection of `ordered`:
/// every mutator calls [`rebuild_positions`](Self::rebuild_positions)
/// before returning, so external observers can never see a stale map.
#[derive(Default, Debug, Clone)]
pub(crate) struct Favorites {
    ordered: Vec<Tz>,
    position: HashMap<Tz, usize>,
}

impl Favorites {
    /// Builds favourites from on-disk string form. Unparseable entries
    /// are skipped silently — matching the prior behaviour where invalid
    /// IANA strings simply dropped out of the loaded list.
    pub(crate) fn from_strings(items: impl IntoIterator<Item = String>) -> Self {
        let ordered: Vec<Tz> = items
            .into_iter()
            .filter_map(|s| s.parse::<Tz>().ok())
            .collect();
        let position = ordered.iter().enumerate().map(|(i, tz)| (*tz, i)).collect();
        Self { ordered, position }
    }

    /// Serializes the ordered list back into IANA strings for persistence.
    pub(crate) fn to_strings(&self) -> Vec<String> {
        self.ordered
            .iter()
            .map(|tz| tz.name().to_string())
            .collect()
    }

    pub(crate) fn contains(&self, tz: Tz) -> bool {
        self.position.contains_key(&tz)
    }

    pub(crate) fn position(&self, tz: Tz) -> Option<usize> {
        self.position.get(&tz).copied()
    }

    /// Returns the raw position map. Exposed so callers like the
    /// [`SearchIndex::search`] hot path can pass `&HashMap<Tz, usize>`
    /// without rebuilding it.
    pub(crate) fn position_map(&self) -> &HashMap<Tz, usize> {
        &self.position
    }

    pub(crate) fn top(&self, n: usize) -> impl Iterator<Item = Tz> + '_ {
        self.ordered.iter().take(n).copied()
    }

    /// Toggles `tz` in the favourite list. Returns `true` when the
    /// timezone was added, `false` when it was removed.
    pub(crate) fn toggle(&mut self, tz: Tz) -> bool {
        if let Some(&idx) = self.position.get(&tz) {
            self.ordered.remove(idx);
            self.rebuild_positions();
            false
        } else {
            self.ordered.push(tz);
            self.position.insert(tz, self.ordered.len() - 1);
            true
        }
    }

    /// Swaps `tz` one slot earlier in the order. Returns `true` if a
    /// move happened, `false` if `tz` was absent or already first.
    pub(crate) fn move_up(&mut self, tz: Tz) -> bool {
        if let Some(idx) = self.position(tz)
            && idx > 0
        {
            self.ordered.swap(idx - 1, idx);
            self.rebuild_positions();
            return true;
        }
        false
    }

    /// Swaps `tz` one slot later in the order. Returns `true` if a
    /// move happened, `false` if `tz` was absent or already last.
    pub(crate) fn move_down(&mut self, tz: Tz) -> bool {
        if let Some(idx) = self.position(tz)
            && idx + 1 < self.ordered.len()
        {
            self.ordered.swap(idx, idx + 1);
            self.rebuild_positions();
            return true;
        }
        false
    }

    fn rebuild_positions(&mut self) {
        self.position = self
            .ordered
            .iter()
            .enumerate()
            .map(|(i, tz)| (*tz, i))
            .collect();
    }
}

/// A single visible row in the filtered timezone table.
///
/// `display_name` may differ from the entry's canonical `city` when
/// the row matched on an alias — e.g. searching `boston` shows
/// `Boston` instead of `New York`.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FilteredRow {
    pub(crate) catalogue_idx: usize,
    pub(crate) display_name: &'static str,
}

/// The current filtered view as a single owned vector of rows.
///
/// Fuses the previously parallel `filtered_indices` / `filtered_display_names`
/// vectors so an index can never be zipped against the wrong label.
#[derive(Default, Debug)]
pub(crate) struct FilteredView {
    rows: Vec<FilteredRow>,
}

impl FilteredView {
    pub(crate) fn rows(&self) -> &[FilteredRow] {
        &self.rows
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn get(&self, idx: usize) -> Option<FilteredRow> {
        self.rows.get(idx).copied()
    }

    /// Populates the view from a sequence of catalogue indices, looking
    /// up each entry's canonical city as the display label.
    pub(crate) fn set_from_indices(
        &mut self,
        indices: impl IntoIterator<Item = usize>,
        catalogue: &Catalogue,
    ) {
        self.rows = indices
            .into_iter()
            .filter_map(|idx| {
                catalogue.get(idx).map(|e| FilteredRow {
                    catalogue_idx: idx,
                    display_name: e.city,
                })
            })
            .collect();
    }

    /// Populates the view from pre-scored search results, where each
    /// hit already carries the alias-aware display name to render.
    pub(crate) fn set_from_scored(
        &mut self,
        scored: impl IntoIterator<Item = (usize, &'static str)>,
    ) {
        self.rows = scored
            .into_iter()
            .map(|(catalogue_idx, display_name)| FilteredRow {
                catalogue_idx,
                display_name,
            })
            .collect();
    }
}

/// Whether the app is accepting navigation keys or search text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

/// The currently-selected timezone and the display name it was selected
/// under.
///
/// Bundling these into a single value forces the two to stay in sync:
/// every selection update goes through [`Selection::set`], so a caller
/// can't assign a new `tz` while forgetting to update `city_name` (which
/// would leave the big-clock label disagreeing with the actual
/// time-zone being displayed).
///
/// `city_name` may differ from the entry's canonical city — when the
/// user picks a row that matched on an alias (e.g. searching "boston"
/// hits the New York entry), the alias label is what they expect to
/// see, not "New York".
#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub(crate) tz: chrono_tz::Tz,
    pub(crate) city_name: String,
}

impl Selection {
    pub(crate) fn set(&mut self, tz: chrono_tz::Tz, city_name: impl Into<String>) {
        self.tz = tz;
        self.city_name = city_name.into();
    }
}

/// Outcome of the most recent clipboard-copy attempt.
///
/// `Failure` carries a short, user-facing message (typically the program
/// name plus the underlying OS error) so the status bar can show what
/// actually went wrong rather than a blank screen.
#[derive(Debug, Clone)]
pub enum CopyStatus {
    Success,
    Failure(String),
}

/// Transient feedback shown in the status bar after pressing `c`.
///
/// The UI auto-clears the flash after a short timeout based on
/// `started_at`; the timeout used to be ~2 seconds for the success-only
/// flow and is kept the same now that errors share the slot.
#[derive(Debug, Clone)]
pub(crate) struct CopyFlash {
    pub(crate) status: CopyStatus,
    pub(crate) started_at: Instant,
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
    pub(crate) should_quit: bool,
    pub(crate) input_mode: InputMode,
    pub(crate) search_query: String,
    /// Byte offset into `search_query` (not char index), because
    /// `String::insert` and `String::remove` operate on byte positions.
    pub(crate) cursor_position: usize,

    /// Immutable timezone catalogue plus its derived indexes. See
    /// [`Catalogue`] for the invariant it enforces.
    pub(crate) catalogue: Catalogue,
    /// Currently visible rows (catalogue index + display label fused
    /// into a single value so the two can never drift). See
    /// [`FilteredView`].
    pub(crate) filtered_view: FilteredView,
    /// Currently highlighted row in `filtered_view`.
    pub(crate) selected_row: usize,

    /// The timezone whose time is shown in the big clock plus the
    /// display label it was selected under. See [`Selection`] for why
    /// the two are coupled.
    pub(crate) selection: Selection,

    pub(crate) theme: Theme,
    /// Ordered favourites with O(1) membership lookup. See [`Favorites`].
    pub(crate) favorites: Favorites,
    pub(crate) show_favorites_only: bool,
    /// Most recent clipboard-copy result. The renderer in
    /// [`crate::ui::draw_status_bar`] compares
    /// `flash.started_at.elapsed()` against a 3-second window each
    /// frame; once that window passes the flash is simply not drawn —
    /// the field itself is overwritten on the next copy attempt rather
    /// than being eagerly cleared by a state-mutation tick. Keeps
    /// `App` allocation-free on the idle tick path.
    pub(crate) copy_flash: Option<CopyFlash>,
    /// When true, the UI renders a help popup over the main view.
    /// Toggled by `?` and dismissed by any key while open.
    pub(crate) show_help: bool,
    /// One-shot messages produced at startup (config-load warnings or
    /// the parse-error message) for the status bar to surface.
    pub(crate) startup_messages: Vec<String>,
    /// `true` when the on-disk config could not be parsed.
    ///
    /// While set, [`App::save_config`] is a no-op — saving would
    /// overwrite the user's broken file before they can fix it. Cleared
    /// only by a successful reload (currently: process restart).
    pub(crate) config_load_failed: bool,
    /// Timestamp captured at construction time. Used by the UI to decide
    /// when to stop showing [`startup_messages`].
    pub(crate) started_at: Instant,
}

impl App {
    pub(crate) fn new() -> Self {
        let (cfg, startup_messages, config_load_failed) = match config::default_path() {
            Some(path) => match config::try_load(&path) {
                Ok((cfg, warnings)) => {
                    let messages = warnings.into_iter().map(|w| w.0).collect();
                    (cfg, messages, false)
                }
                Err(e) => (
                    config::Config::default(),
                    vec![format!("Config load failed: {e}")],
                    true,
                ),
            },
            None => (config::Config::default(), Vec::new(), false),
        };
        Self::with_config_state(cfg, startup_messages, config_load_failed)
    }

    #[cfg(test)]
    fn with_config(cfg: config::Config) -> Self {
        Self::with_config_state(cfg, Vec::new(), false)
    }

    fn with_config_state(
        cfg: config::Config,
        startup_messages: Vec<String>,
        config_load_failed: bool,
    ) -> Self {
        let catalogue = Catalogue::new();
        let theme = Theme::from_label(&cfg.theme);
        let favorites = Favorites::from_strings(cfg.favorites);

        let mut indices: Vec<usize> = (0..catalogue.len()).collect();
        Self::sort_indices(&mut indices, catalogue.entries(), favorites.position_map());
        let mut filtered_view = FilteredView::default();
        filtered_view.set_from_indices(indices, &catalogue);

        Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            cursor_position: 0,
            catalogue,
            filtered_view,
            selected_row: 0,
            selection: Selection {
                tz: chrono_tz::Tz::UTC,
                city_name: "UTC".to_string(),
            },
            theme,
            favorites,
            show_favorites_only: false,
            copy_flash: None,
            show_help: false,
            startup_messages,
            config_load_failed,
            started_at: Instant::now(),
        }
    }

    /// Dismisses the startup-message banner.
    ///
    /// Called from [`crate::events::dispatch_key`] on every keypress so
    /// the banner is gone the moment the user starts interacting with
    /// the app — the 10-second auto-timeout in the renderer is the
    /// fallback when the user simply watches the screen.
    pub(crate) fn dismiss_startup_messages(&mut self) {
        if !self.startup_messages.is_empty() {
            self.startup_messages.clear();
        }
    }

    pub(crate) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub(crate) fn close_help(&mut self) {
        self.show_help = false;
    }

    pub(crate) fn move_up(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
    }

    pub(crate) fn move_down(&mut self) {
        if !self.filtered_view.is_empty() && self.selected_row < self.filtered_view.len() - 1 {
            self.selected_row += 1;
        }
    }

    pub(crate) fn page_up(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(10);
    }

    pub(crate) fn page_down(&mut self) {
        if !self.filtered_view.is_empty() {
            self.selected_row = (self.selected_row + 10).min(self.filtered_view.len() - 1);
        }
    }

    pub(crate) fn home(&mut self) {
        self.selected_row = 0;
    }

    pub(crate) fn end(&mut self) {
        self.selected_row = self.filtered_view.len().saturating_sub(1);
    }

    pub(crate) fn select_timezone(&mut self) {
        if let Some((idx, display_name)) = self.current_result()
            && let Some(entry) = self.catalogue.get(idx)
        {
            self.selection.set(entry.tz, display_name);
        }
    }

    pub(crate) fn enter_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.cursor_position = 0;
    }

    pub(crate) fn exit_search(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub(crate) fn search_input(&mut self, c: char) {
        self.search_query.insert(self.cursor_position, c);
        self.cursor_position += c.len_utf8();
        self.apply_filter();
        self.select_first_result();
    }

    /// Inserts a bulk pasted string at the cursor in one shot.
    ///
    /// Control characters (other than tab, which is converted to a
    /// single space) are stripped — terminals frequently embed stray
    /// `\r` or escape sequences in paste payloads. All other Unicode
    /// scalars are preserved so CJK, accented Latin, and copied IANA
    /// IDs paste verbatim.
    ///
    /// Filters are recomputed once after the entire bulk insert
    /// (not per-character) to keep paste latency O(query).
    pub(crate) fn search_paste(&mut self, text: &str) {
        let sanitised: String = text
            .chars()
            .filter_map(|c| {
                if c == '\t' {
                    Some(' ')
                } else if c.is_control() {
                    None
                } else {
                    Some(c)
                }
            })
            .collect();
        if sanitised.is_empty() {
            return;
        }
        self.search_query
            .insert_str(self.cursor_position, &sanitised);
        self.cursor_position += sanitised.len();
        self.apply_filter();
        self.select_first_result();
    }

    /// Commits the currently highlighted search result, then exits
    /// search mode.
    ///
    /// Enter in search mode is the "I want this one" path — it both
    /// picks the row and leaves the input box, so the user doesn't have
    /// to press Enter twice. Esc remains the "I changed my mind" path
    /// and only calls [`exit_search`](Self::exit_search).
    ///
    /// Safe when the filtered view is empty: nothing is selected and
    /// the call degrades to a plain `exit_search`.
    pub(crate) fn commit_search_result_and_exit(&mut self) {
        if !self.filtered_view.is_empty() {
            self.select_timezone();
        }
        self.exit_search();
    }

    /// Deletes from `cursor_position` back to the start of the previous
    /// word (readline `Ctrl-w` semantics).
    ///
    /// Walks past trailing whitespace first, then past the run of
    /// non-whitespace before it — so `asia +9 ` followed by `Ctrl-w`
    /// removes `+9 ` and leaves `asia `, matching the bash convention
    /// users expect.
    pub(crate) fn delete_word_before_cursor(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        let mut end = self.cursor_position;
        // Skip trailing whitespace immediately before the cursor.
        while end > 0 {
            let prev_char = self.search_query[..end].chars().next_back();
            match prev_char {
                Some(c) if c.is_whitespace() => end -= c.len_utf8(),
                _ => break,
            }
        }
        // Skip the run of non-whitespace before the whitespace gap.
        while end > 0 {
            let prev_char = self.search_query[..end].chars().next_back();
            match prev_char {
                Some(c) if !c.is_whitespace() => end -= c.len_utf8(),
                _ => break,
            }
        }
        self.search_query.drain(end..self.cursor_position);
        self.cursor_position = end;
        self.apply_filter();
        self.select_first_result();
    }

    /// Truncates the query at the cursor (readline `Ctrl-k` semantics).
    pub(crate) fn delete_to_end_of_line(&mut self) {
        if self.cursor_position < self.search_query.len() {
            self.search_query.truncate(self.cursor_position);
            self.apply_filter();
            self.select_first_result();
        }
    }

    pub(crate) fn search_backspace(&mut self) {
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

    pub(crate) fn clear_search_input(&mut self) {
        self.search_query.clear();
        self.cursor_position = 0;
        self.apply_filter();
        self.select_first_result();
    }

    /// Moves the search cursor one Unicode scalar to the left (byte-aware).
    ///
    /// True grapheme-cluster movement would require the
    /// `unicode-segmentation` crate; not worth the dep for timezone
    /// queries.
    pub(crate) fn search_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            let prev = self.search_query[..self.cursor_position]
                .chars()
                .next_back()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_position -= prev;
        }
    }

    /// Moves the search cursor one Unicode scalar to the right (byte-aware).
    ///
    /// True grapheme-cluster movement would require the
    /// `unicode-segmentation` crate; not worth the dep for timezone
    /// queries.
    pub(crate) fn search_cursor_right(&mut self) {
        if self.cursor_position < self.search_query.len() {
            let next = self.search_query[self.cursor_position..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_position += next;
        }
    }

    pub(crate) fn search_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    pub(crate) fn search_cursor_end(&mut self) {
        self.cursor_position = self.search_query.len();
    }

    /// Deletes the character at the cursor (forward delete).
    pub(crate) fn search_delete(&mut self) {
        if self.cursor_position < self.search_query.len() {
            self.search_query.remove(self.cursor_position);
            self.apply_filter();
            self.select_first_result();
        }
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
    pub(crate) fn apply_filter(&mut self) {
        let base_indices = self.base_indices();

        if self.search_query.is_empty() {
            self.set_sorted_results(base_indices);
        } else {
            let now = Utc::now();
            match self.catalogue.search_index().search(
                &self.search_query,
                self.catalogue.entries(),
                &base_indices,
                &now,
                self.favorites.position_map(),
            ) {
                // The query normalized to no usable terms (e.g. pure
                // punctuation) — fall back to the unfiltered, sorted
                // view as if the box were empty.
                None => self.set_sorted_results(base_indices),
                Some(scored) => self.filtered_view.set_from_scored(
                    scored
                        .into_iter()
                        .map(|(idx, display_name, _)| (idx, display_name)),
                ),
            }
        }
        self.clamp_selected_row();
    }

    pub(crate) fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        self.save_config();
    }

    pub(crate) fn toggle_favorites_filter(&mut self) {
        self.show_favorites_only = !self.show_favorites_only;
        self.apply_filter();
    }

    pub(crate) fn toggle_favorite(&mut self) {
        if let Some((idx, _)) = self.current_result()
            && let Some(entry) = self.catalogue.get(idx)
        {
            let tz = entry.tz;
            self.favorites.toggle(tz);
            self.save_config();
            self.apply_filter();
        }
    }

    pub(crate) fn move_favorite_up(&mut self) {
        if let Some((idx, _)) = self.current_result()
            && let Some(entry) = self.catalogue.get(idx)
            && self.favorites.move_up(entry.tz)
        {
            self.save_config();
            self.apply_filter();
            self.selected_row = self.selected_row.saturating_sub(1);
        }
    }

    pub(crate) fn move_favorite_down(&mut self) {
        if let Some((idx, _)) = self.current_result()
            && let Some(entry) = self.catalogue.get(idx)
            && self.favorites.move_down(entry.tz)
        {
            self.save_config();
            self.apply_filter();
            if self.selected_row + 1 < self.filtered_view.len() {
                self.selected_row += 1;
            }
        }
    }

    pub(crate) fn top_favorite_timezones(&self, count: usize) -> Vec<(chrono_tz::Tz, &str)> {
        self.favorites
            .top(count)
            .filter_map(|tz| {
                let idx = self.catalogue.by_tz(tz)?;
                Some((tz, self.catalogue.get(idx)?.city))
            })
            .collect()
    }

    pub(crate) fn is_favorite(&self, tz_index: usize) -> bool {
        self.catalogue
            .get(tz_index)
            .is_some_and(|entry| self.favorites.contains(entry.tz))
    }

    fn current_result(&self) -> Option<(usize, &'static str)> {
        let row = self.filtered_view.get(self.selected_row)?;
        Some((row.catalogue_idx, row.display_name))
    }

    fn base_indices(&self) -> Vec<usize> {
        let entries = self.catalogue.entries();
        if self.show_favorites_only {
            (0..entries.len())
                .filter(|&i| self.favorites.contains(entries[i].tz))
                .collect()
        } else {
            (0..entries.len()).collect()
        }
    }

    fn set_sorted_results(&mut self, mut indices: Vec<usize>) {
        Self::sort_indices(
            &mut indices,
            self.catalogue.entries(),
            self.favorites.position_map(),
        );
        self.filtered_view
            .set_from_indices(indices, &self.catalogue);
    }

    fn clamp_selected_row(&mut self) {
        if self.filtered_view.is_empty() {
            self.selected_row = 0;
        } else if self.selected_row >= self.filtered_view.len() {
            self.selected_row = self.filtered_view.len() - 1;
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

    fn save_config(&mut self) {
        // Refuse to save while the on-disk config is known to be broken
        // — overwriting it would destroy the user's only copy of the
        // file they need to fix. They'll see the parse error in the
        // status bar at startup and can correct it manually.
        if self.config_load_failed {
            return;
        }
        let Some(path) = config::default_path() else {
            return;
        };
        let cfg = config::Config {
            theme: self.theme.label().to_string(),
            favorites: self.favorites.to_strings(),
        };
        // Route through `try_save` (not `save`) so we can surface the
        // failure to the user. The original `config::save` eprintln!s on
        // error — invisible inside the alternate screen — so we forward
        // the message through `startup_messages` to the status bar
        // instead. Don't block subsequent saves; only the most recent
        // failure is retained.
        if let Err(e) = config::try_save(&path, &cfg) {
            let msg = format!("Config save failed: {e}");
            // Replace any prior save-error so the status bar shows the
            // most recent failure rather than stale text.
            self.startup_messages
                .retain(|m| !m.starts_with("Config save failed:"));
            self.startup_messages.push(msg);
            // Reset the timer so the new save error is visible for the
            // full 10-second window, even if startup was minutes ago.
            self.started_at = Instant::now();
        }
    }

    /// Copies the selected timezone's current time to the system
    /// clipboard in compact ISO-ish format (e.g. `20260603T1430+0900`).
    ///
    /// The format string `%Y%m%dT%H%M%z` produces a date+time stamp
    /// followed by the numeric UTC offset (`+0900`), not a tz
    /// abbreviation — abbreviations like `CST` are ambiguous between
    /// Central US and China Standard Time, so we deliberately avoid
    /// `%Z`.
    ///
    /// Uses platform-specific clipboard tools:
    /// - macOS: `pbcopy`
    /// - Windows: `clip`
    /// - Linux: `wl-copy` (Wayland) with `xclip` fallback (X11)
    ///
    /// Records the outcome in [`Self::copy_flash`] — success when any
    /// candidate succeeds, failure (carrying the last underlying error
    /// message) when every candidate failed. The platform with no
    /// configured clipboard tool reports a specific message rather than
    /// silently doing nothing.
    pub(crate) fn copy_time(&mut self) {
        let now = Utc::now().with_timezone(&self.selection.tz);
        let formatted = now.format("%Y%m%dT%H%M%z").to_string();
        self.copy_flash = Some(CopyFlash {
            status: match clipboard::copy(&formatted) {
                Ok(()) => CopyStatus::Success,
                Err(msg) => CopyStatus::Failure(msg),
            },
            started_at: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — assertions, expect, and explicit
    // panic!("…") are how a failing test reports its failure to the runner.
    // Re-enabling the production lints here would force `if let Some(_) = …`
    // boilerplate that obscures the actual invariant under test.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

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
        app.filtered_view
            .rows()
            .iter()
            .take(count)
            .map(|row| app.catalogue.get(row.catalogue_idx).unwrap().city)
            .collect()
    }

    fn filtered_display_names(app: &App, count: usize) -> Vec<&'static str> {
        app.filtered_view
            .rows()
            .iter()
            .take(count)
            .map(|row| row.display_name)
            .collect()
    }

    fn first_city(app: &App) -> &'static str {
        let row = app.filtered_view.get(0).expect("filtered_view is empty");
        app.catalogue.get(row.catalogue_idx).unwrap().city
    }

    fn first_tz(app: &App) -> chrono_tz::Tz {
        let row = app.filtered_view.get(0).expect("filtered_view is empty");
        app.catalogue.get(row.catalogue_idx).unwrap().tz
    }

    fn nth_tz(app: &App, n: usize) -> chrono_tz::Tz {
        let row = app
            .filtered_view
            .get(n)
            .unwrap_or_else(|| panic!("filtered_view has no row {n}"));
        app.catalogue.get(row.catalogue_idx).unwrap().tz
    }

    #[test]
    fn search_matches_iana_timezone_ids() {
        let mut app = test_app();

        apply_query(&mut app, "america/new_york");

        assert_eq!(first_city(&app), "New York");
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
        assert_eq!(first_city(&app), "Chicago");
        assert_eq!(filtered_display_names(&app, 1), vec!["Texas"]);

        app.select_timezone();
        assert_eq!(app.selection.city_name, "Texas");

        apply_query(&mut app, "eastern time");
        assert_eq!(first_city(&app), "New York");
        assert_eq!(filtered_display_names(&app, 1), vec!["New York"]);
    }

    #[test]
    fn search_displays_the_matching_alias_city() {
        let mut app = test_app();

        apply_query(&mut app, "boston");

        assert_eq!(first_city(&app), "New York");
        assert_eq!(filtered_display_names(&app, 1), vec!["Boston"]);

        app.select_timezone();
        assert_eq!(app.selection.city_name, "Boston");
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
        let last = app.filtered_view.len() - 1;
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
        let last = app.filtered_view.len() - 1;
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
        assert_eq!(app.selected_row, app.filtered_view.len() - 1);
    }

    #[test]
    fn end_on_empty_list_is_safe() {
        let mut app = test_app();
        // Force an empty filter result
        apply_query(&mut app, "zzzzzznotaquery");
        assert!(app.filtered_view.is_empty());

        app.end();
        assert_eq!(app.selected_row, 0);
    }

    #[test]
    fn navigation_on_empty_list_is_safe() {
        let mut app = test_app();
        apply_query(&mut app, "zzzzzznotaquery");
        assert!(app.filtered_view.is_empty());

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
        let tz = app.selection.tz;

        assert!(!app.favorites.contains(tz));
        app.toggle_favorite();
        assert!(app.favorites.contains(tz));
        assert!(app.favorites.position(tz).is_some());

        app.toggle_favorite();
        assert!(!app.favorites.contains(tz));
        assert!(app.favorites.position(tz).is_none());
    }

    #[test]
    fn favorites_filter_shows_only_favorites() {
        let mut app = test_app();
        let total = app.filtered_view.len();

        // Add one favorite
        app.selected_row = 0;
        app.select_timezone();
        app.toggle_favorite();
        assert_eq!(app.filtered_view.len(), total);

        // Toggle favorites-only mode
        app.toggle_favorites_filter();
        assert_eq!(app.filtered_view.len(), 1);

        // Toggle back
        app.toggle_favorites_filter();
        assert_eq!(app.filtered_view.len(), total);
    }

    #[test]
    fn favorites_appear_first_in_sort_order() {
        let mut app = test_app();
        // Navigate to a city that wouldn't normally be first alphabetically
        apply_query(&mut app, "tokyo");
        app.select_timezone();
        let tokyo_idx = app.filtered_view.get(0).unwrap().catalogue_idx;

        // Clear search and toggle favorite
        app.enter_search();
        app.exit_search();
        app.apply_filter();

        // Find Tokyo in the unfiltered list — shouldn't be first
        let pos_before = app
            .filtered_view
            .rows()
            .iter()
            .position(|row| row.catalogue_idx == tokyo_idx)
            .unwrap();
        assert!(pos_before > 0);

        // Favorite it
        app.selected_row = pos_before;
        app.toggle_favorite();

        // Now Tokyo should be first
        assert_eq!(app.filtered_view.get(0).unwrap().catalogue_idx, tokyo_idx);
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
        assert_eq!(app.selection.tz, chrono_tz::Tz::UTC);

        apply_query(&mut app, "tokyo");
        app.select_timezone();
        assert_eq!(app.selection.tz, chrono_tz::Tz::Asia__Tokyo);
        assert_eq!(app.selection.city_name, "Tokyo");
    }

    /// Regression test: prior to commit b4b2ccf, `move_favorite_up`
    /// and `move_favorite_down` swapped entries in `favorites` but did
    /// not rebuild the position lookup, so the next sort used stale
    /// positions and the visual reorder did not take effect.
    ///
    /// After the cluster refactor this invariant is structural — the
    /// position map lives inside [`Favorites`] and is rebuilt on every
    /// mutator — but the behaviour assertion below still pins down the
    /// public-visible sort order, so a regression in the new code path
    /// would still trip the test.
    #[test]
    fn reorder_favorites_updates_sort_order_immediately() {
        let mut app = test_app();

        let favs = [
            ("tokyo", chrono_tz::Tz::Asia__Tokyo),
            ("london", chrono_tz::Tz::Europe__London),
            ("paris", chrono_tz::Tz::Europe__Paris),
        ];
        for (query, _) in favs {
            apply_query(&mut app, query);
            app.toggle_favorite();
        }

        apply_query(&mut app, "");
        assert_eq!(first_tz(&app), favs[0].1);
        assert_eq!(nth_tz(&app, 1), favs[1].1);
        assert_eq!(nth_tz(&app, 2), favs[2].1);

        app.selected_row = 0;
        app.move_favorite_down();

        assert_eq!(first_tz(&app), favs[1].1);
        assert_eq!(nth_tz(&app, 1), favs[0].1);
        assert_eq!(nth_tz(&app, 2), favs[2].1);
        assert_eq!(app.favorites.position(favs[0].1), Some(1));
        assert_eq!(app.favorites.position(favs[1].1), Some(0));

        app.selected_row = 2;
        app.move_favorite_up();

        assert_eq!(first_tz(&app), favs[1].1);
        assert_eq!(nth_tz(&app, 1), favs[2].1);
        assert_eq!(nth_tz(&app, 2), favs[0].1);
    }

    // ── Search editing (paste, commit, word-delete, kill-line) ──────

    #[test]
    fn search_paste_appends_at_cursor_and_runs_filter() {
        let mut app = test_app();
        app.enter_search();
        app.search_input('t');
        app.search_input('o');
        // Cursor at end ("to"). Paste finishes "kyo" — including a
        // control char that must be stripped and a CJK char that must
        // be preserved.
        app.search_paste("k\u{0007}yo東");
        assert_eq!(app.search_query, "tokyo東");
        assert_eq!(app.cursor_position, "tokyo東".len());
        // apply_filter ran exactly once after the bulk insert and the
        // selection snapped back to the first result.
        assert_eq!(app.selected_row, 0);
        // The CJK suffix is non-matching, but the search still
        // executed — verify by clearing the suffix via Ctrl-k and
        // checking that Tokyo is the first hit.
        app.cursor_position = "tokyo".len();
        app.delete_to_end_of_line();
        assert_eq!(first_city(&app), "Tokyo");
    }

    #[test]
    fn commit_search_result_and_exit_picks_highlighted_row() {
        let mut app = test_app();
        app.enter_search();
        for c in "tokyo".chars() {
            app.search_input(c);
        }
        assert_eq!(app.input_mode, InputMode::Search);
        app.commit_search_result_and_exit();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.selection.tz, chrono_tz::Tz::Asia__Tokyo);
        assert_eq!(app.selection.city_name, "Tokyo");
    }

    #[test]
    fn commit_search_result_and_exit_is_safe_when_empty() {
        let mut app = test_app();
        let original_tz = app.selection.tz;
        app.enter_search();
        for c in "zzzzzznotaquery".chars() {
            app.search_input(c);
        }
        assert!(app.filtered_view.is_empty());
        app.commit_search_result_and_exit();
        // No commit happened, but we still left search mode.
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.selection.tz, original_tz);
    }

    #[test]
    fn delete_word_before_cursor_handles_trailing_spaces() {
        let mut app = test_app();
        app.enter_search();
        for c in "asia +9 ".chars() {
            app.search_input(c);
        }
        // Cursor at end. Ctrl-w should drop the trailing space AND
        // the word "+9", leaving "asia ".
        app.delete_word_before_cursor();
        assert_eq!(app.search_query, "asia ");
        assert_eq!(app.cursor_position, "asia ".len());
    }

    #[test]
    fn delete_word_before_cursor_at_zero_is_noop() {
        let mut app = test_app();
        app.enter_search();
        // Empty query, cursor at zero — must not panic or mutate.
        app.delete_word_before_cursor();
        assert_eq!(app.search_query, "");
        assert_eq!(app.cursor_position, 0);

        // Non-empty query but cursor parked at 0 — same invariant.
        for c in "asia".chars() {
            app.search_input(c);
        }
        app.cursor_position = 0;
        app.delete_word_before_cursor();
        assert_eq!(app.search_query, "asia");
        assert_eq!(app.cursor_position, 0);
    }

    #[test]
    fn delete_to_end_of_line_truncates() {
        let mut app = test_app();
        app.enter_search();
        for c in "tokyo+9".chars() {
            app.search_input(c);
        }
        // Park cursor between "tokyo" and "+9", then kill to end.
        app.cursor_position = "tokyo".len();
        app.delete_to_end_of_line();
        assert_eq!(app.search_query, "tokyo");
        assert_eq!(app.cursor_position, "tokyo".len());
        // Filter re-ran: Tokyo is the top hit again.
        assert_eq!(first_city(&app), "Tokyo");
    }

    #[test]
    fn clear_search_input_from_normal_mode_resets_filter_without_changing_mode() {
        // Models the Ctrl-l Normal-mode binding: a user typed a query,
        // pressed Esc (which leaves the filter in place), then hit Ctrl-l
        // to drop the filter without re-entering search mode.
        let mut app = test_app();
        let baseline_view_len = app.filtered_view.len();

        apply_query(&mut app, "tokyo");
        assert!(app.filtered_view.len() < baseline_view_len);
        app.input_mode = InputMode::Normal;

        app.clear_search_input();

        assert_eq!(app.search_query, "");
        assert_eq!(app.cursor_position, 0);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.filtered_view.len(), baseline_view_len);
    }
}
