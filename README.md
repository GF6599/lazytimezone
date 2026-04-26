# lazytimezone

A terminal UI for browsing world clocks across 50+ timezones, built with Rust and [Ratatui](https://ratatui.rs).

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

## Features

- **Big clock display** with large ASCII-art time, city name, and date
- **50 timezones** spanning Pacific, Americas, Europe, Africa, Asia, and Australia
- **Real-time search** by city, region, or UTC offset (e.g. `+5:30`, `UTC-8`, `asia +9`)
- **6 built-in themes** — Default, Dracula, Solarized, Nord, Monokai, Gruvbox — persisted across sessions
- **Time diff column** showing offset from your selected timezone
- **Favorites** — pin timezones to the top of the list, persisted across sessions
- **Favorite side clocks** — up to 2 favorite timezones shown as smaller clocks beside the main clock
- **Clipboard copy** of the selected timezone's current time

## Installation

### From source

```sh
# Build release binary
cargo build --release

# Or using just
just install   # builds and copies binary to ~/
```

### Requirements

- Rust 2024 edition (1.85+)
- macOS (clipboard copy uses `pbcopy`)

## Usage

```sh
lazytimezone
```

### Keybindings

#### Normal mode

| Key | Action |
|---|---|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `g` / `Home` | Jump to top |
| `G` / `End` | Jump to bottom |
| `Ctrl-d` / `Page Down` | Page down |
| `Ctrl-u` / `Page Up` | Page up |
| `Enter` | Select timezone (updates clock and diff column) |
| `f` | Toggle favorite on selected timezone |
| `F` | Toggle favorites-only filter |
| `J` | Move favorite down in order |
| `K` | Move favorite up in order |
| `/` | Enter search mode |
| `t` | Cycle theme |
| `c` | Copy time to clipboard |
| `?` | Toggle help overlay |
| `q` / `Ctrl-c` | Quit |

#### Search mode

| Key | Action |
|---|---|
| Type | Filter timezones |
| `←` / `→` | Move cursor |
| `Home` / `End` | Jump to start / end (also `Ctrl-a` / `Ctrl-e`) |
| `Backspace` | Delete previous character |
| `Delete` | Delete character under cursor |
| `Ctrl-u` | Clear search |
| `Esc` / `Enter` | Exit search |

Search is case-insensitive with AND logic across whitespace-separated terms. It ignores punctuation, understands IANA timezone IDs, and also indexes common area keywords such as state names and timezone-family labels. Offset searches use each timezone's current UTC offset, so DST-aware cities move seasonally.

When a search hits an alias city or a displayable geographic keyword, the table shows that matched label even if the underlying timezone entry is grouped under a representative city.

| Query | Matches |
|---|---|
| `london` | London |
| `asia` | All Asian timezones |
| `america/new_york` | New York |
| `boston` | Boston (America/New_York) |
| `st johns` | `St. John's` entries |
| `texas` | Texas (mapped to U.S. Central Time) |
| `eastern time` | New York / Toronto |
| `+5:30` | UTC+5:30 (Mumbai) |
| `+0530` | UTC+5:30 (Mumbai, Colombo) |
| `UTC-10` | UTC-10 timezones (Honolulu) |
| `GMT-10:00` | UTC-10 timezones (Honolulu) |
| `united states` | USA timezones |
| `asia +9` | Asian cities at UTC+9 (Tokyo, Seoul) |

## Themes

Cycle through themes with `t`. Theme and favorites are saved to `~/.config/lazytimezone/config.toml`.

- **Default** — terminal colors with cyan accents
- **Dracula** — purple/blue dark theme
- **Solarized** — high-contrast light/dark
- **Nord** — arctic blue palette
- **Monokai** — classic editor colors
- **Gruvbox** — retro groove palette

## Architecture

```
src/
├── main.rs       Entry point and TUI event loop
├── app.rs        Core state: selection, search, favorites, theme
├── events.rs     Keyboard input → App mutations
├── ui.rs         App state → ratatui Frame rendering
├── config.rs     TOML persistence (~/.config/lazytimezone/config.toml)
├── theme.rs      Colour palettes
└── timezone.rs   Static catalogue of 50+ world cities
```

The application follows a single-owner state model: `App` holds all mutable state, the event loop calls `events::handle_events` to mutate it, then `ui::draw` reads it to render each frame. No shared or global state.

## Development

```sh
just run          # run the app
just fmt          # format + clippy
just dist         # build release to dist/
just install      # build + copy to ~/
```

## License

MIT
