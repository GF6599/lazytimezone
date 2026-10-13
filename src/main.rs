//! # lazytimezone — A terminal world-clock browser.
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
/// preserved — the TUI disappears cleanly when the app quits.
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

        // Tick rate matches the clock's display granularity (1 s). An
        // idle loop sleeps until the next second boundary, which keeps
        // battery use low, and a key wakes it at once.
        let tick_rate = std::time::Duration::from_secs(1);
        let mut last_tick = std::time::Instant::now();

        loop {
            terminal.draw(|frame| {
                ui::draw(frame, &mut app);
            })?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            events::handle_events(&mut app, &mut events, timeout)?;

            if last_tick.elapsed() >= tick_rate {
                // Advance by exactly one tick_rate instead of snapping to
                // Instant::now() — this preserves alignment to the real
                // second boundary even if poll wakeup added latency.
                last_tick += tick_rate;
                // Catch-up guard: if we fell more than one tick behind
                // (e.g. the laptop slept), snap forward rather than
                // spinning to replay missed ticks.
                if last_tick.elapsed() > tick_rate {
                    last_tick = std::time::Instant::now();
                }
            }

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
