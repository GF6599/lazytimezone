//! # Keyboard event handling.
//!
//! Maps raw crossterm key events to [`App`] method calls. The module is
//! intentionally thin. All business logic lives in [`App`], and this layer
//! only decides *which* method to call.
//!
//! Key bindings are split by [`InputMode`]:
//! - **Normal**: vim-style navigation, theme cycling, favorites, clipboard
//! - **Search**: readline-style text entry, plus Ctrl-f to favourite the
//!   highlighted row without leaving the query

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::{App, InputMode};

/// The terminal is the only implementation that ships. The seam exists
/// because the drain loop below is otherwise unreachable under test:
/// `event::poll` needs a live terminal.
pub trait EventSource {
    fn poll(&mut self, timeout: Duration) -> std::io::Result<bool>;
    fn read(&mut self) -> std::io::Result<Event>;
}

#[derive(Debug)]
pub struct TerminalEvents;

impl EventSource for TerminalEvents {
    fn poll(&mut self, timeout: Duration) -> std::io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> std::io::Result<Event> {
        event::read()
    }
}

/// Without a cap the drain below only exits when the producer runs out,
/// so piped or held input stalls the redraw and the quit check.
const MAX_DRAINED_EVENTS: usize = 256;

/// Polls for terminal events (keys, pastes and resizes) for at most
/// `timeout`.
///
/// The caller (the event loop in `main`) computes `timeout` so the
/// poll wakes for the next frame of the main clock.
///
/// Only `KeyEventKind::Press` is handled. Release and repeat events
/// are ignored to prevent duplicate actions on platforms that emit them.
pub fn handle_events(
    app: &mut App,
    source: &mut impl EventSource,
    timeout: Duration,
) -> std::io::Result<()> {
    if !source.poll(timeout)? {
        return Ok(());
    }
    dispatch_event(app, source.read()?);

    // Coalesce auto-repeat bursts: when the user holds j/k, crossterm
    // queues a key event per repeat. Draining the already-ready ones here
    // means the main loop redraws once per burst, not once per key.
    let mut drained = 0;
    while drained < MAX_DRAINED_EVENTS && source.poll(Duration::ZERO)? {
        dispatch_event(app, source.read()?);
        drained += 1;
    }
    Ok(())
}

/// No arm may break out of the caller's drain. `source.read` has already
/// dequeued the event, so leaving early drops it: harmless for a
/// `Resize`, silent data loss for a `Paste`.
fn dispatch_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) => dispatch_key(app, key),
        // Bracketed paste: in Search mode, splat the entire payload
        // into the query in one shot (avoiding the per-char filter
        // recompute). Outside search there's no text field to paste into.
        Event::Paste(text) => {
            if app.input_mode == InputMode::Search {
                app.search_paste(&text);
            }
        }
        // Mouse / focus events are not bound to any action yet. Match
        // them explicitly so future additions are an obvious diff
        // rather than a silently-dropped event.
        Event::Resize(_, _) | Event::Mouse(_) | Event::FocusGained | Event::FocusLost => {}
    }
}

/// Routes a single key event through the Ctrl-C, help-modal, and
/// mode-specific handlers.
fn dispatch_key(app: &mut App, key: crossterm::event::KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }
    // Any keypress dismisses the startup-message banner so it never
    // lingers past the user's first interaction. Done up here (not
    // inside the per-mode handlers) so EVERY key flushes the banner,
    // including ones that would otherwise be no-ops. The keypress is
    // still allowed to flow through to its normal handler below.
    app.dismiss_startup_messages();
    // Help is modal: the arrows scroll it, any other key dismisses it
    // before reaching the mode-specific handlers below.
    if app.show_help {
        match key.code {
            KeyCode::Up => app.scroll_help(-1),
            KeyCode::Down => app.scroll_help(1),
            KeyCode::PageUp => app.scroll_help(-10),
            KeyCode::PageDown => app.scroll_help(10),
            _ => app.close_help(),
        }
        return;
    }
    match app.input_mode {
        InputMode::Normal => handle_normal_mode(app, key),
        InputMode::Search => handle_search_mode(app, key),
    }
}

/// A key held with Ctrl or Alt. Shift is not a chord, because it is
/// how `G`, `J`, `K` and `?` arrive.
fn is_chord(key: crossterm::event::KeyEvent) -> bool {
    key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

/// Wall bindings: `h/j/k/l` move between the favorite panels, Enter
/// promotes one to the hero clock, `f` removes it, `J/K` reorder, `m`
/// switches to the world map and back, and `/` opens the add-city
/// picker.
fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    // The picker binds Ctrl-f, and the same chord here must not remove
    // a favourite.
    if is_chord(key) {
        return;
    }
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => app.panel_down(),
        KeyCode::Char('k') | KeyCode::Up => app.panel_up(),
        KeyCode::Char('h') | KeyCode::Left => app.panel_left(),
        KeyCode::Char('l') | KeyCode::Right => app.panel_right(),
        KeyCode::Char('g') | KeyCode::Home => app.panel_first(),
        KeyCode::Char('G') | KeyCode::End => app.panel_last(),
        KeyCode::Enter => app.promote_selected_panel(),
        KeyCode::Char('/') => app.enter_search(),
        KeyCode::Char('f') => app.unfavorite_selected_panel(),
        KeyCode::Char('J') => app.move_panel_later(),
        KeyCode::Char('K') => app.move_panel_earlier(),
        KeyCode::Char('t') => app.cycle_theme(),
        KeyCode::Char('m') => app.toggle_map(),
        KeyCode::Char('a') => app.toggle_clock_animation(),
        KeyCode::Char('c') => app.copy_time(),
        KeyCode::Char('?') => app.toggle_help(),
        _ => {}
    }
}

/// Text-input bindings: printable chars insert at cursor, Backspace
/// deletes the previous char, Delete removes the char under the
/// cursor, Left/Right move the cursor, Home/End jump to ends,
/// Ctrl-u clears, Ctrl-w deletes the previous word, Ctrl-k deletes to
/// end of line, Ctrl-f toggles favourite on the highlighted row, Esc
/// exits without committing, Enter commits the highlighted row and
/// exits.
///
/// Ctrl ordering matters: the Ctrl-modified arms must come before the
/// catch-all `KeyCode::Char(c)` insert, since a bare `f`/`w`/`k`/`u`
/// keystroke should still type a literal character into the query.
fn handle_search_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => app.exit_search(),
        KeyCode::Enter => app.commit_search_result_and_exit(),
        KeyCode::Backspace => app.search_backspace(),
        KeyCode::Delete => app.search_delete(),
        KeyCode::Up => app.select_previous_result(),
        KeyCode::Down => app.select_next_result(),
        KeyCode::Left => app.search_cursor_left(),
        KeyCode::Right => app.search_cursor_right(),
        KeyCode::Home => app.search_cursor_home(),
        KeyCode::End => app.search_cursor_end(),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_search_input();
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_cursor_home();
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_cursor_end();
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.delete_word_before_cursor();
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.delete_to_end_of_line();
        }
        // Ctrl-f toggles favourite on the highlighted row without
        // leaving search mode. Bare `f` keeps inserting the literal
        // character, so multi-word queries like `fiji` still work.
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_favorite();
        }
        KeyCode::Char(c) if !is_chord(key) => app.search_input(c),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design. See src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use std::collections::VecDeque;

    use crossterm::event::{KeyEvent, KeyEventState};

    use super::*;
    use crate::app::WallView;
    use crate::config;

    /// `poll` reports readiness without blocking, which is what a burst
    /// of already-buffered terminal input looks like.
    struct QueuedEvents {
        queued: VecDeque<Event>,
    }

    impl QueuedEvents {
        fn new(events: Vec<Event>) -> Self {
            Self {
                queued: events.into(),
            }
        }

        fn remaining(&self) -> usize {
            self.queued.len()
        }
    }

    impl EventSource for QueuedEvents {
        fn poll(&mut self, _timeout: Duration) -> std::io::Result<bool> {
            Ok(!self.queued.is_empty())
        }

        fn read(&mut self) -> std::io::Result<Event> {
            self.queued
                .pop_front()
                .ok_or_else(|| std::io::Error::other("polled ready with an empty queue"))
        }
    }

    fn key(c: char) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn chord(c: char, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn searching_app() -> App {
        let mut app = App::with_config(config::Config::default());
        app.enter_search();
        app
    }

    /// `m` switches the wall to the world map and back.
    ///
    /// Given: the app on the panel wall.
    /// When: `m` is pressed, then pressed again.
    /// Then: the first press shows the map and the second the panels.
    ///
    /// Why it matters: the map has no other way in, so a key that is in
    /// the help but not wired would hide the whole view.
    #[test]
    fn m_switches_the_wall_to_the_map_and_back() {
        // Given:
        let mut app = App::with_config(config::Config::default());

        // When:
        dispatch_event(&mut app, key('m'));
        let after_one_press = app.wall_view;
        dispatch_event(&mut app, key('m'));

        // Then:
        assert_eq!(
            (after_one_press, app.wall_view),
            (WallView::Map, WallView::Panels)
        );
    }

    /// `a` turns the clock animation off and on.
    ///
    /// Given: the app with its animation on, as it starts by default.
    /// When: `a` is pressed twice.
    /// Then: the first press turns the animation off and the second
    /// turns it on.
    ///
    /// Why it matters: the key is the only way to turn the animation
    /// off without an edit to the config file.
    #[test]
    fn a_turns_the_clock_animation_off_and_on() {
        // Given:
        let mut app = App::with_config(config::Config::default());

        // When:
        dispatch_event(&mut app, key('a'));
        let after_one_press = app.clock_animation;
        dispatch_event(&mut app, key('a'));

        // Then:
        assert_eq!((after_one_press, app.clock_animation), (false, true));
    }

    /// A Ctrl or Alt chord on the wall runs no command.
    ///
    /// Given: one favourite on the wall.
    /// When: Ctrl-f, Ctrl-t, Ctrl-q and Alt-f are pressed.
    /// Then: the favourite, the theme and the app all stay.
    ///
    /// Why it matters: Ctrl-f favourites a row in the picker, so a
    /// hand that repeats it on the wall must not remove a favourite and
    /// save the config.
    #[test]
    fn a_ctrl_or_alt_chord_on_the_wall_runs_no_command() {
        // Given:
        let mut app = searching_app();
        for c in "tokyo".chars() {
            dispatch_event(&mut app, key(c));
        }
        app.commit_search_result_and_exit();
        assert_eq!(app.favorites.len(), 1);
        let theme = app.theme;

        // When:
        for (c, modifiers) in [
            ('f', KeyModifiers::CONTROL),
            ('t', KeyModifiers::CONTROL),
            ('q', KeyModifiers::CONTROL),
            ('f', KeyModifiers::ALT),
        ] {
            dispatch_event(&mut app, chord(c, modifiers));
        }

        // Then:
        assert_eq!(app.favorites.len(), 1);
        assert_eq!(app.theme, theme);
        assert!(!app.should_quit);
    }

    /// An unbound Ctrl or Alt chord in the picker types nothing, and a
    /// shifted letter still types.
    ///
    /// Given: the picker with an empty query.
    /// When: Ctrl-p and Alt-n are pressed, then Shift-T.
    /// Then: the query is `T`.
    ///
    /// Why it matters: Ctrl-p and Ctrl-n move through lists in other
    /// tools, and typing their letter would refilter the list instead.
    #[test]
    fn an_unbound_chord_in_the_picker_types_nothing() {
        // Given:
        let mut app = searching_app();

        // When:
        dispatch_event(&mut app, chord('p', KeyModifiers::CONTROL));
        dispatch_event(&mut app, chord('n', KeyModifiers::ALT));
        dispatch_event(&mut app, chord('T', KeyModifiers::SHIFT));

        // Then:
        assert_eq!(app.search_query, "T");
    }

    /// Ctrl-u in the picker clears the query and brings back the browse
    /// list.
    ///
    /// Given: the picker with `tokyo` typed.
    /// When: Ctrl-u is pressed.
    /// Then: the query is empty, the picker is still open, and the list
    /// is the one an empty picker shows.
    ///
    /// Why it matters: Ctrl-u is the readline key for starting over, and
    /// a stale filter after it would hide most cities.
    #[test]
    fn ctrl_u_in_the_picker_clears_the_query_and_the_filter() {
        // Given:
        let browse_rows = searching_app().filtered_view.len();
        let mut app = searching_app();
        for c in "tokyo".chars() {
            dispatch_event(&mut app, key(c));
        }
        assert_ne!(app.filtered_view.len(), browse_rows);

        // When:
        dispatch_event(&mut app, chord('u', KeyModifiers::CONTROL));

        // Then:
        assert_eq!(app.search_query, "");
        assert_eq!(app.input_mode, InputMode::Search);
        assert_eq!(app.filtered_view.len(), browse_rows);
    }

    #[test]
    fn a_paste_arriving_behind_a_keystroke_is_not_discarded() {
        let mut app = searching_app();
        let mut source = QueuedEvents::new(vec![key('a'), Event::Paste("sia".to_string())]);

        handle_events(&mut app, &mut source, Duration::ZERO).unwrap();

        assert_eq!(app.search_query, "asia");
    }

    #[test]
    fn a_resize_behind_a_keystroke_does_not_strand_the_rest_of_the_burst() {
        let mut app = searching_app();
        let mut source =
            QueuedEvents::new(vec![key('a'), Event::Resize(80, 24), key('b'), key('c')]);

        handle_events(&mut app, &mut source, Duration::ZERO).unwrap();

        assert_eq!(app.search_query, "abc");
    }

    #[test]
    fn the_drain_yields_before_an_unbounded_burst_starves_the_redraw() {
        // Held `j` is the realistic burst: crossterm queues one event
        // per auto-repeat, and navigation skips the filter recompute.
        let mut app = App::with_config(config::Config::default());
        let flood: Vec<Event> = (0..10_000).map(|_| key('j')).collect();
        let mut source = QueuedEvents::new(flood);

        handle_events(&mut app, &mut source, Duration::ZERO).unwrap();

        assert!(
            source.remaining() > 0,
            "the loop must hand control back so the frame can be redrawn"
        );
    }

    #[test]
    fn an_idle_poll_leaves_the_app_untouched() {
        let mut app = searching_app();
        let mut source = QueuedEvents::new(vec![]);

        handle_events(&mut app, &mut source, Duration::ZERO).unwrap();

        assert_eq!(app.search_query, "");
    }
}
