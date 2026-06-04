//! # Keyboard event handling.
//!
//! Maps raw crossterm key events to [`App`] method calls. The module is
//! intentionally thin — all business logic lives in [`App`]; this layer
//! only decides *which* method to call.
//!
//! Key bindings are split by [`InputMode`]:
//! - **Normal**: vim-style navigation, theme cycling, favorites, clipboard
//! - **Search**: text input with backspace and clear (Ctrl-u)

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::{App, InputMode};

/// Polls for a key event for at most `timeout`.
///
/// The caller (the event loop in `main`) computes `timeout` so the
/// poll wakes at the next tick boundary, keeping clock updates tight
/// (≤ tick) while drawing only once per tick when idle.
///
/// Only `KeyEventKind::Press` is handled — release and repeat events
/// are ignored to prevent duplicate actions on platforms that emit them.
pub fn handle_events(app: &mut App, timeout: Duration) -> std::io::Result<()> {
    if !event::poll(timeout)? {
        return Ok(());
    }
    match event::read()? {
        Event::Key(key) => {
            dispatch_key(app, key);
            // Coalesce auto-repeat bursts: when the user holds j/k, crossterm
            // queues a key event per repeat. Drain any already-ready key events
            // here so the main loop redraws once per burst, not once per key.
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    Event::Key(next) => dispatch_key(app, next),
                    // Non-Key events during a burst: stop draining so the main
                    // loop can react (e.g. a Resize) on its next iteration.
                    _ => break,
                }
            }
        }
        // Resize is handled implicitly by the main loop's unconditional
        // redraw; the explicit arm documents that we consume it on purpose.
        Event::Resize(_, _) => {}
        // Mouse / focus events are not bound to any action yet. Match
        // them explicitly so future additions are an obvious diff
        // rather than a silently-dropped event.
        Event::Mouse(_) | Event::FocusGained | Event::FocusLost => {}
        // Bracketed paste: in Search mode, splat the entire payload
        // into the query in one shot (avoiding the per-char filter
        // recompute). Outside search there's no text field to paste
        // into, so the event is dropped — same as the prior behaviour.
        Event::Paste(text) => {
            if app.input_mode == InputMode::Search {
                app.search_paste(&text);
            }
        }
    }
    Ok(())
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
    // Help is modal: any key dismisses it before reaching the
    // mode-specific handlers below.
    if app.show_help {
        app.close_help();
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
