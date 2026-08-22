//! # Keyboard event handling.
//!
//! Maps raw crossterm key events to [`App`] method calls. The module is
//! intentionally thin — all business logic lives in [`App`]; this layer
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

/// Polls for a key event for at most `timeout`.
///
/// The caller (the event loop in `main`) computes `timeout` so the
/// poll wakes at the next tick boundary, keeping clock updates tight
/// (≤ tick) while drawing only once per tick when idle.
///
/// Only `KeyEventKind::Press` is handled — release and repeat events
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
/// mode-specific handlers. Extracted so `handle_events` can also call it
/// when draining a coalesced key-repeat burst.
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
    // inside the per-mode handlers) so EVERY key flushes the banner —
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

/// Vim-style bindings: `j/k` navigate, `/` searches, `f` toggles
/// favorites, `J/K` reorder favorites, `t` cycles themes, `c` copies,
/// `Ctrl-l` clears any active search filter without entering search
/// mode (shell-readline convention).
fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('g') | KeyCode::Home => app.home(),
        KeyCode::Char('G') | KeyCode::End => app.end(),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => app.page_down(),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => app.page_up(),
        // Ctrl-l: clear any active filter while staying in Normal mode.
        // Esc only exits search mode without wiping the query, so without
        // this binding the only way to reset a stale filter is `/` (which
        // also re-enters search mode).
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_search_input();
        }
        KeyCode::PageDown => app.page_down(),
        KeyCode::PageUp => app.page_up(),
        KeyCode::Char('/') => app.enter_search(),
        KeyCode::Char('t') => app.cycle_theme(),
        KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => app.copy_time(),
        KeyCode::Char('f') => app.toggle_favorite(),
        KeyCode::Char('F') => app.toggle_favorites_filter(),
        KeyCode::Char('J') => app.move_favorite_down(),
        KeyCode::Char('K') => app.move_favorite_up(),
        KeyCode::Enter => app.select_timezone(),
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
        KeyCode::Char(c) => app.search_input(c),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use std::collections::VecDeque;

    use crossterm::event::{KeyEvent, KeyEventState};

    use super::*;
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

    fn searching_app() -> App {
        let mut app = App::with_config(config::Config::default());
        app.enter_search();
        app
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
