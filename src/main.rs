//! # lazytimezone — A terminal world-clock browser.
//!
//! ## Architecture
//!
//! The application follows a classic TUI loop pattern:
//!
//! ```text
//! main ─► run_tui() ─► loop { draw → handle_events → check quit }
//!              │
//!              ├── app       Core state: selection, search, favorites, theme
//!              ├── events    Keyboard input → App mutations
//!              ├── ui        App state → ratatui Frame rendering
//!              ├── config    TOML persistence (~/.config/lazytimezone/config.toml)
//!              ├── theme     Color palettes
//!              └── timezone  Static catalogue of 220+ world cities
//! ```
//!
//! State flows unidirectionally: [`events`] mutates [`app::App`], then
//! [`ui::draw`] reads it to produce the next frame. No shared or global
//! state — everything lives in the single [`app::App`] instance.

mod app;
mod config;
mod events;
mod theme;
mod timezone;
mod ui;

fn main() -> std::io::Result<()> {
    run_tui()
}

/// Initialises the terminal, runs the event loop, and restores the terminal
/// on exit.
///
/// Uses crossterm's alternate screen so the user's scrollback buffer is
/// preserved — the TUI disappears cleanly when the app quits.
///
/// ## Errors
///
/// Propagates I/O errors from terminal setup, rendering, or event polling.
fn run_tui() -> std::io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    // Mouse capture is intentionally NOT enabled: we have no mouse
    // handlers, and enabling it would break native terminal text
    // selection (users would have to hold Shift/Option to select).
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = app::App::new();

    loop {
        terminal.draw(|frame| {
            ui::draw(frame, &mut app);
        })?;

        events::handle_events(&mut app)?;

        if app.should_quit {
            break;
        }
    }

    // Restore terminal to its original state regardless of how the loop ended.
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    Ok(())
}
