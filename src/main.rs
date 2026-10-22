//! # lazytimezone: a terminal world-clock browser.
//!
//! ## Architecture
//!
//! The application follows a classic TUI loop pattern:
//!
//! ```text
//! main ─► run_tui() ─► loop { draw → handle_events → check quit }
//!              │
//!              ├── cli        Command-line arguments
//!              ├── app        Core state: selection, search, favorites, theme
//!              ├── events     Keys, pastes and resizes → App mutations
//!              ├── ui         App state → ratatui Frame rendering
//!              ├── digits     The hero clock's block digits
//!              ├── sky        The sky band above the wall
//!              ├── search     Search index and scoring
//!              ├── clipboard  The copy key's clipboard tools
//!              ├── config     TOML persistence (see `cli` for the path)
//!              ├── theme      Color palettes
//!              └── timezone   Generated catalogue of 34k world cities
//! ```
//!
//! State flows unidirectionally: [`events`] mutates [`app::App`], then
//! [`ui::draw`] reads it to produce the next frame. Mutable state all
//! lives in the single [`app::App`] instance; the catalogue and its
//! search index are immutable statics.

mod app;
mod cli;
mod clipboard;
mod config;
mod digits;
mod events;
mod land;
mod octant;
mod search;
mod sky;
mod theme;
mod timezone;
mod ui;

use std::path::PathBuf;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = run_tui(cli.config_path()) {
        eprintln!("lazytimezone: {e}");
        std::process::exit(1);
    }
}

/// Initialises the terminal, runs the event loop, and restores the terminal
/// on exit.
///
/// Uses crossterm's alternate screen so the user's scrollback buffer is
/// preserved, and the TUI disappears cleanly when the app quits.
///
/// ## Errors
///
/// Propagates I/O errors from terminal setup, rendering, or event polling.
fn run_tui(config_path: Option<PathBuf>) -> std::io::Result<()> {
    // Mouse capture stays off: the app has no mouse handlers, and capture
    // would break the terminal's own text selection. try_init can fail
    // after it has turned raw mode on, so its error path restores too.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            ratatui::restore();
            return Err(std::io::Error::new(
                e.kind(),
                format!("cannot open the terminal: {e}"),
            ));
        }
    };
    // ratatui's panic hook leaves raw mode and the alternate screen only.
    let restore_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        disable_bracketed_paste();
        restore_hook(info);
    }));

    // Every `?` below lands in `result`, so the restore after the closure
    // runs on every exit.
    let result = (|| -> std::io::Result<()> {
        // Bracketed paste delivers a paste as one Event::Paste, so
        // App::search_paste filters once per paste, not once per character.
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste)?;

        let mut app = app::App::new(config_path);
        let mut events = events::TerminalEvents;

        // With the animation off the loop wakes once a second, which keeps
        // battery use low, and a key wakes it at once.
        loop {
            terminal.draw(|frame| {
                ui::draw(frame, &mut app);
            })?;

            let timeout = ui::until_next_frame(chrono::Utc::now(), app.clock_moves());
            events::handle_events(&mut app, &mut events, timeout)?;

            if app.should_quit {
                break;
            }
        }

        Ok(())
    })();

    disable_bracketed_paste();
    ratatui::restore();
    result
}

/// Turns bracketed paste off. A failure is ignored: the caller is already
/// leaving, and the error that made it leave is the one worth reporting.
fn disable_bracketed_paste() {
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);
}
