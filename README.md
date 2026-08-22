# lazytimezone

A terminal UI for browsing world clocks across 217 cities, built with Rust and [Ratatui](https://ratatui.rs).

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

## Features

- **Big clock display** with large ASCII-art time, city name, and date
- **217 cities** spanning Pacific, Americas, Europe, Africa, Asia, and Australia
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
just ship   # builds and copies the binary to ~/self-made-bin/
```

### Requirements

- Rust 2024 edition (1.85+)
- Clipboard copy shells out to a platform tool, which must be on `PATH`:
  `pbcopy` on macOS, `clip` on Windows, `wl-copy` or `xclip` on Linux.
  Everything else works without them.
- `just build` additionally needs [`cross`](https://github.com/cross-rs/cross)
  for the Linux target and `codesign` for the macOS binary.

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
| `Ctrl-l` | Clear active search filter |
| `t` | Cycle theme |
| `c` | Copy time to clipboard |
| `?` | Toggle help overlay (`↑` / `↓` scroll it) |
| `q` / `Ctrl-c` | Quit |

#### Search mode

| Key | Action |
|---|---|
| Type | Filter timezones |
| `←` / `→` | Move cursor |
| `Home` / `End` | Jump to start / end (also `Ctrl-a` / `Ctrl-e`) |
| `Backspace` | Delete previous character |
| `Delete` | Delete character under cursor |
| `Ctrl-w` | Delete previous word |
| `Ctrl-k` | Delete to end of line |
| `Ctrl-u` | Clear search |
| `Ctrl-f` | Toggle favorite on the highlighted row |
| `Enter` | Pick the highlighted row and exit search |
| `Esc` | Exit search without picking |

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

The application follows a single-owner state model: `App` holds all mutable state, the event loop calls `events::handle_events` to mutate it, then `ui::draw` reads it to render each frame. No shared or global state.

## Development

```sh
just              # list every recipe
just run          # run the app
just check        # the full gate: clippy, tests, formatting
just fmt          # format in place
just build        # build release binaries into dist/
just ship         # build, then copy to ~/self-made-bin/
```

`just check` is what CI runs. Run it before pushing.

## License

MIT
