# 🕰️ lazytimezone

A terminal world clock browser. Search by city, region, country, or UTC offset, then read the time as a large clock.

[![ci](https://github.com/GF6599/lazytimezone/actions/workflows/ci.yml/badge.svg)](https://github.com/GF6599/lazytimezone/actions/workflows/ci.yml)

## Features

- **Large clock.** The time, the city, and the date, in block digits.
- **217 cities.** The Pacific, the Americas, Europe, Africa, Asia, and Australia.
- **Search.** By city, region, country, or UTC offset, for example `+5:30`, `UTC-8`, `asia +9`.
- **6 themes.** The app saves your choice.
- **Time difference column.** The offset from the city you select.
- **Favorites.** Pin a city to the top of the list. The app saves the order.
- **Side clocks.** Up to 2 favorites show as smaller clocks beside the main clock.
- **Clipboard copy.** Copy the current time of the selected city.

## Install

You must have Rust 1.85 or later, because the crate uses the 2024 edition.

```sh
cargo build --release
```

To install the binary, first make the target directory, then build and copy in one step. `~/self-made-bin` must be on your `PATH`.

```sh
mkdir -p ~/self-made-bin
just ship
```

`just ship` builds a macOS binary and a Linux binary, so it also needs
[`cross`](https://github.com/cross-rs/cross) and `codesign`. Use
`cargo build --release` to build for your own platform only.

Clipboard copy calls a platform tool, which must be on `PATH`: `pbcopy` on macOS, `clip` on Windows, `wl-copy` or `xclip` on Linux. Every other feature works without one.

## Usage

```sh
lazytimezone
```

Press `?` in the app for the same key list, plus the search syntax.

### Normal mode

| Key | Action |
|---|---|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `g` / `Home` | Jump to top |
| `G` / `End` | Jump to bottom |
| `Ctrl-d` / `Page Down` | Page down |
| `Ctrl-u` / `Page Up` | Page up |
| `Enter` | Select the city, which sets the main clock and the difference column |
| `f` | Toggle favorite on the selected city |
| `F` | Toggle the favorites-only filter |
| `J` | Move the favorite down in the order |
| `K` | Move the favorite up in the order |
| `/` | Enter search mode |
| `Ctrl-l` | Clear the active search filter |
| `t` | Cycle the theme |
| `c` | Copy the time to the clipboard |
| `?` | Toggle the help overlay. `Up` and `Down` scroll it |
| `q` / `Ctrl-c` | Quit |

### Search mode

| Key | Action |
|---|---|
| Type | Filter the cities |
| `Left` / `Right` | Move the cursor |
| `Home` / `End` | Jump to the start or the end, also `Ctrl-a` / `Ctrl-e` |
| `Backspace` | Delete the previous character |
| `Delete` | Delete the character under the cursor |
| `Ctrl-w` | Delete the previous word |
| `Ctrl-k` | Delete to the end of the line |
| `Ctrl-u` | Clear the search |
| `Ctrl-f` | Toggle favorite on the highlighted city |
| `Enter` | Pick the highlighted city and exit the search |
| `Esc` | Exit the search without a pick |

### Search syntax

Search is not case-sensitive. It applies AND logic across the terms that whitespace separates, and it ignores punctuation. It accepts an IANA timezone identifier, and it also indexes area keywords such as a state name or a timezone-family label.

An offset search uses the current UTC offset of each city. A city that observes daylight saving time therefore moves between the seasons.

A search can match an alias city or a geographic keyword. The table then shows the label that matched, even when the entry is grouped under a representative city.

| Query | Matches |
|---|---|
| `london` | London |
| `asia` | All Asian timezones |
| `america/new_york` | New York |
| `boston` | Boston, which is `America/New_York` |
| `st johns` | The `St. John's` entries |
| `texas` | Texas, which maps to United States Central Time |
| `eastern time` | New York and Toronto |
| `+5:30` | The UTC+5:30 timezones, such as Mumbai |
| `+0530` | The UTC+5:30 timezones, such as Mumbai and Colombo |
| `UTC-10` | The UTC-10 timezones, such as Honolulu |
| `GMT-10:00` | The UTC-10 timezones, such as Honolulu |
| `united states` | The United States timezones |
| `asia +9` | The Asian cities at UTC+9, such as Tokyo and Seoul |

## Configuration

Press `t` to cycle the theme. The cycle order is Default, Dracula, Solarized, Nord, Monokai, and Gruvbox.

The app writes the theme and the favorites to `~/.config/lazytimezone/config.toml`. When `$XDG_CONFIG_HOME` is set, the app writes to `$XDG_CONFIG_HOME/lazytimezone/config.toml` instead.

## Development

```sh
just          # list every recipe
just check    # the quality gate: clippy, then the tests, then a format check
```

`just check` is what CI runs, so run it before you push. To run the same checks on every commit, install the hooks once with `pre-commit install`.

## License

MIT.
