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
    if event::poll(timeout)?
        && let Event::Key(key) = event::read()?
    {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return Ok(());
        }
        match app.input_mode {
            InputMode::Normal => handle_normal_mode(app, key),
            InputMode::Search => handle_search_mode(app, key),
        }
    }
    Ok(())
}

/// Vim-style bindings: `j/k` navigate, `/` searches, `f` toggles
/// favorites, `J/K` reorder favorites, `t` cycles themes, `c` copies.
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
        _ => {}
    }
}

/// Text-input bindings: printable chars insert at cursor, Backspace
/// deletes the previous char, Delete removes the char under the
/// cursor, Left/Right move the cursor, Home/End jump to ends,
/// Ctrl-u clears, Esc/Enter exits back to normal mode.
fn handle_search_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.exit_search(),
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
        KeyCode::Char(c) => app.search_input(c),
        _ => {}
    }
}
