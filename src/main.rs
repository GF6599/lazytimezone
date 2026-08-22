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
//!              └── timezone  Static catalogue of 217 world cities
//! ```
//!
//! State flows unidirectionally: [`events`] mutates [`app::App`], then
//! [`ui::draw`] reads it to produce the next frame. No shared or global
//! state — everything lives in the single [`app::App`] instance.

mod app;
mod clipboard;
mod config;
mod events;
mod search;
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
    // ratatui::init() enables raw mode, enters the alternate screen, AND
    // installs a panic hook that calls ratatui::restore() before unwinding,
    // so a panic inside ui::draw can no longer leave the user's shell
    // corrupted. Mouse capture is intentionally NOT enabled: we have no
    // mouse handlers, and enabling it would break native terminal text
    // selection (users would have to hold Shift/Option to select).
    let mut terminal = ratatui::init();

    // Opt into bracketed paste so the terminal delivers pasted text as a
    // single Event::Paste payload instead of N synthetic keypresses —
    // App::search_paste relies on this to run apply_filter exactly once
    // per paste rather than per character.
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste)?;

    // Wrap the loop in a closure so cleanup runs unconditionally — any
    // `?` propagation from terminal.draw or handle_events flows through
    // `result` instead of skipping the restore() call below.
    let result = (|| -> std::io::Result<()> {
        let mut app = app::App::new();
        let mut events = events::TerminalEvents;

        // Tick rate matches the clock's display granularity (1 s).
        // Idle iterations sleep until the next second boundary instead of
        // polling at 20 Hz, which keeps battery usage low. Key events
        // wake the loop immediately so input feels instant.
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

    // Disable bracketed paste before ratatui::restore() drops us out of
    // the alternate screen. We deliberately ignore the result here so
    // the genuine `result` from the loop closure is what the caller
    // sees — restoring the terminal must take priority over a paste-mode
    // toggle failure.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);

    // Restore terminal to its original state regardless of how the loop ended.
    ratatui::restore();
    result
}
