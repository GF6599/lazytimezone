//! # Terminal UI rendering.
//!
//! Pure rendering layer — reads [`App`] state and writes to a ratatui
//! [`Frame`]. No input handling or state mutation happens here (aside
//! from reading `App` fields and borrowing `&mut` for table state).
//!
//! ## Layout (top to bottom)
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │ Title bar (1 row)                            │
//! ├──────────────────────────────────────────────┤
//! │ Hero clock: block-digit time, city, date     │
//! ├──────────────────────────────────────────────┤
//! │ Favorite wall: one framed panel per favorite │
//! │  ┌ Tokyo ─────┐ ┌ London ────┐               │
//! ├──────────────────────────────────────────────┤
//! │ Status bar (1 row)                           │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! The hero clock adapts to the terminal: the block digits step up
//! in scale as the terminal grows, shrink to half height on a
//! mid-size one, and drop to a plain text line where no art fits.
//! The favorite wall stretches its
//! panel grid to the frame edges and gives each panel more rows when
//! the height allows. The add-city picker renders as a centered
//! modal over the wall while search mode is active.

use chrono::offset::Offset;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use chrono_tz::Tz;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Alignment,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::canvas::{Canvas, Map, MapResolution, Points},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::app::{App, CopyStatus, InputMode, WallView};
use crate::digits;
use crate::sky;
use crate::theme::ThemeColors;
use crate::timezone::{
    Coordinates, TimezoneEntry, format_utc_offset, is_daytime_at, is_daytime_at_coordinates,
    sun_times,
};

/// Top-level render entry point — splits the frame into five vertical
/// zones and delegates to specialised draw functions.
pub fn draw(frame: &mut Frame, app: &mut App) {
    draw_at(frame, app, Utc::now());
}

/// Draws the frame as it looks at `now`. Every child draw takes this one
/// instant, so every part of the UI agrees on "now", and a test can pick
/// the instant.
fn draw_at(frame: &mut Frame, app: &mut App, now: DateTime<Utc>) {
    let tc = app.theme.colors();

    let (hero, hero_height) = hero_variant(frame.area());
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // title bar
            Constraint::Length(hero_height), // hero clock
            Constraint::Min(0),              // favorite wall
            Constraint::Length(1),           // status bar
        ])
        .split(frame.area());

    draw_title_bar(frame, app, outer[0], &tc);
    draw_hero_clock(frame, app, &now, outer[1], hero, &tc);
    match app.wall_view {
        WallView::Panels => draw_wall(frame, app, &now, outer[2], &tc),
        WallView::Map => draw_map(frame, app, &now, outer[2], &tc),
    }
    draw_status_bar(frame, app, outer[3], &tc);

    if app.input_mode == InputMode::Search {
        draw_picker(frame, app, &now, frame.area(), &tc);
    }
    if app.show_help {
        draw_help_popup(frame, app, frame.area(), &tc);
    }
}

/// Renders a centered help popup listing every keybinding, grouped
/// by mode. Scrolled with the arrows, dismissed by any other keypress
/// (handled in `events.rs`).
///
/// Uses [`Clear`] to wipe the underlying cells before drawing, so
/// the popup's transparent borders don't bleed background through.
/// The README lists the same bindings for a reader who has not built the
/// app yet. Both lists are hand-written, so
/// `every_help_binding_appears_in_the_readme` holds them together.
const NORMAL_MODE_HELP: &[(&str, &str)] = &[
    ("h / j / k / l", "Move between panels (also the arrows)"),
    ("g / G", "Jump to first / last panel"),
    ("Enter", "Show the panel's city on the big clock"),
    ("/", "Open the add-city search"),
    ("f", "Remove the selected panel"),
    ("J / K", "Move the panel later / earlier in the order"),
    ("t", "Cycle theme"),
    ("m", "Switch the wall between panels and the world map"),
    ("c", "Copy the big clock's time to clipboard"),
    ("?", "Toggle this help"),
    ("q / Ctrl-c", "Quit"),
];

const SEARCH_MODE_HELP: &[(&str, &str)] = &[
    ("Type", "Filter the cities (AND across terms)"),
    ("Up / Down", "Move through the results"),
    ("Left / Right", "Move cursor"),
    ("Home / End", "Jump to start / end (also Ctrl-a / Ctrl-e)"),
    ("Backspace / Delete", "Delete previous / next char"),
    ("Ctrl-w", "Delete previous word"),
    ("Ctrl-k", "Delete to end of line"),
    ("Ctrl-u", "Clear search"),
    ("Ctrl-f", "Toggle favorite on highlighted row"),
    ("Enter", "Add the highlighted city and close"),
    ("Esc", "Close without adding"),
];

/// Bold accent heading that opens each of the overlay's three sections.
fn section_header(title: &'static str, tc: &ThemeColors) -> Line<'static> {
    Line::from(Span::styled(
        title,
        Style::default().fg(tc.accent).add_modifier(Modifier::BOLD),
    ))
}

fn draw_help_popup(frame: &mut Frame, app: &mut App, area: Rect, tc: &ThemeColors) {
    let mut lines: Vec<Line> = vec![section_header("Normal mode", tc)];
    lines.extend(
        NORMAL_MODE_HELP
            .iter()
            .map(|&(key, desc)| help_row(key, desc, tc)),
    );
    lines.push(Line::from(""));
    lines.push(section_header("Search mode", tc));
    lines.extend(
        SEARCH_MODE_HELP
            .iter()
            .map(|&(key, desc)| help_row(key, desc, tc)),
    );
    lines.push(Line::from(""));
    lines.push(section_header("Search syntax", tc));
    lines.extend([
        syntax_row("tokyo", "city or alias", tc),
        syntax_row("+5:30, UTC-8", "offset (today's local time)", tc),
        syntax_row("asia, europe", "region", tc),
        syntax_row("united states", "country", tc),
        syntax_row("eastern time", "timezone phrase", tc),
        syntax_row("asia +9", "two terms (AND)", tc),
        Line::from(Span::styled(
            "  Tip: offsets reflect each city's CURRENT local time (DST-adjusted).",
            Style::default().fg(tc.muted).add_modifier(Modifier::ITALIC),
        )),
    ]);

    let popup_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    // 76 fits the longest row ("Tip: offsets reflect...") without
    // truncation; `centered_rect` clamps it on a narrower terminal.
    let popup_width: u16 = 76;
    let popup = centered_rect(popup_width, popup_height, area);

    // The overlay is taller than a short terminal, so the tail is only
    // reachable by scrolling. Clamping lives here because this is the
    // only place the popup's height is known.
    let visible = popup.height.saturating_sub(2) as usize;
    let max_scroll = (lines.len().saturating_sub(visible)) as u16;
    app.clamp_help_scroll(max_scroll);

    frame.render_widget(Clear, popup);

    let footer = if max_scroll > 0 {
        " \u{2191}\u{2193} scroll \u{00b7} any other key to close "
    } else {
        " any key to close "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tc.accent))
        .title(" Help ")
        .title_style(Style::default().fg(tc.title).add_modifier(Modifier::BOLD))
        .title_bottom(Line::from(Span::styled(footer, Style::default().fg(tc.muted))).centered());

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.help_scroll, 0))
        .style(Style::default().fg(tc.fg).bg(tc.bg));
    frame.render_widget(paragraph, popup);
}

/// Two-column help row: key on the left (fixed-width, bold), description on the right (muted).
fn help_row(key: &'static str, desc: &'static str, tc: &ThemeColors) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<20}", key),
            Style::default().fg(tc.good).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(desc, Style::default().fg(tc.muted)),
    ])
}

/// Two-column search-syntax row: query example on the left (fixed-width,
/// accent-coloured to read as "code"), description on the right (muted).
fn syntax_row(example: &'static str, desc: &'static str, tc: &ThemeColors) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<16}", example), Style::default().fg(tc.info)),
        Span::raw("  "),
        Span::styled(desc, Style::default().fg(tc.muted)),
    ])
}

/// Centers a rect of `(w, h)` inside `area`, clamping to fit.
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

fn draw_title_bar(frame: &mut Frame, app: &App, area: Rect, tc: &ThemeColors) {
    // Two-cell horizontal split: left flexes, right is exactly the
    // theme label plus a trailing space. ratatui handles truncation
    // and padding — no manual width arithmetic required.
    let theme_text = format!("{} ", app.theme.label());
    let theme_text_width = theme_text.as_str().width() as u16;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(theme_text_width)])
        .split(area);

    let title = Paragraph::new(Span::styled(
        " lazytimezone",
        Style::default().fg(tc.title).add_modifier(Modifier::BOLD),
    ))
    .style(Style::default().bg(tc.bg).fg(tc.fg));
    frame.render_widget(title, chunks[0]);

    let theme = Paragraph::new(Span::styled(theme_text, Style::default().fg(tc.muted)))
        .alignment(ratatui::layout::Alignment::Right)
        .style(Style::default().bg(tc.bg));
    frame.render_widget(theme, chunks[1]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeroVariant {
    Art { rows: u16 },
    Text,
}

/// The hero art ladder, largest first: the rows the digits take, then
/// the terminal height the rung needs. Height leaves the wall at least
/// two panel rows, except the smallest rung, which squeezes the wall to
/// one row rather than dropping the art entirely.
const HERO_LADDER: [(u16, u16); 4] = [(21, 35), (14, 28), (7, 21), (4, 14)];

/// Every time of day draws at one width, so any time can size the art.
const CLOCK_SAMPLE: &str = "00:00:00";

/// Picks the largest hero clock the terminal can carry, returning the
/// variant and the rows the hero zone claims from the vertical layout.
/// A rung fits when the art leaves one margin cell on each side.
fn hero_variant(area: Rect) -> (HeroVariant, u16) {
    for &(rows, min_height) in &HERO_LADDER {
        if area.width >= digits::width(CLOCK_SAMPLE, rows) + 2 && area.height >= min_height {
            return (HeroVariant::Art { rows }, rows + 4);
        }
    }
    (HeroVariant::Text, 4)
}

fn clock_colour(app: &App, utc_now: &DateTime<Utc>, tc: &ThemeColors) -> Color {
    if is_daytime_at(
        app.main_clock_place(),
        &utc_now.with_timezone(&app.selection.tz),
    ) {
        tc.accent
    } else {
        tc.accent_secondary
    }
}

/// Renders the hero clock at the size [`hero_variant`] chose.
///
/// ## Day/night colouring
///
/// [`clock_colour`] takes `accent` by day and `accent_secondary` by
/// night, from the sun at the main clock's own city. So a city such as
/// Reykjavík stays night through a winter noon twilight, and day
/// through a polar summer night.
fn draw_hero_clock(
    frame: &mut Frame,
    app: &App,
    utc_now: &DateTime<Utc>,
    area: Rect,
    variant: HeroVariant,
    tc: &ThemeColors,
) {
    let now = utc_now.with_timezone(&app.selection.tz);
    let clock_color = clock_colour(app, utc_now, tc);

    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%A, %B %d, %Y").to_string();
    let offset_secs = now.offset().fix().local_minus_utc();
    let meta = format!(
        "{} \u{b7} {} \u{b7} {}",
        app.selection.city_name,
        date_str,
        format_utc_offset(offset_secs)
    );

    let HeroVariant::Art { rows } = variant else {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                time_str,
                Style::default()
                    .fg(clock_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(meta, Style::default().fg(tc.muted))),
        ];
        let p = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().bg(tc.bg));
        frame.render_widget(p, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),    // gap under the title bar
            Constraint::Length(rows), // digits
            Constraint::Length(1),    // gap
            Constraint::Length(1),    // meta line
        ])
        .split(area);

    let art_width = digits::width(&time_str, rows);
    let clock_area = Rect {
        x: chunks[1].x + chunks[1].width.saturating_sub(art_width) / 2,
        width: art_width.min(chunks[1].width),
        ..chunks[1]
    };

    let art_style = Style::default().fg(clock_color).bg(tc.bg);
    let art_lines: Vec<Line> = digits::render(&time_str, rows)
        .into_iter()
        .map(|row| Line::from(Span::styled(row, art_style)))
        .collect();
    let art = Paragraph::new(art_lines).style(Style::default().bg(tc.bg));
    frame.render_widget(art, clock_area);

    let meta_line = Paragraph::new(Line::from(Span::styled(
        meta,
        Style::default().fg(tc.muted),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(tc.bg));
    frame.render_widget(meta_line, chunks[3]);
}

/// One gutter column and two border columns leave 24 cells inside, one
/// for each hour of the day strip.
const PANEL_MIN_WIDTH: u16 = 27;
const PANEL_MIN_HEIGHT: u16 = 4;
/// Tall enough for every panel line: time, offset, zone, sun times and
/// the day strip.
const PANEL_MAX_HEIGHT: u16 = 7;
const STRIP_HOURS: u16 = 24;

/// Renders the favorite wall: one framed panel per favorite, in the
/// user's order, wrapped into as many columns as the width allows.
/// The grid geometry is reported back to the app so `j`/`k` know how
/// far one vertical step moves.
///
/// Columns share the width remainder so the grid reaches the right
/// edge, panels grow toward [`PANEL_MAX_HEIGHT`] when the rows fit,
/// and a grid that fits entirely centers itself vertically.
fn draw_wall(
    frame: &mut Frame,
    app: &mut App,
    utc_now: &DateTime<Utc>,
    area: Rect,
    tc: &ThemeColors,
) {
    // One cell of left margin lines the wall up with the hero text,
    // and each grid cell keeps one gutter column so panel frames do
    // not weld into a single rule.
    let available = area.width.saturating_sub(1);
    let columns = (available / PANEL_MIN_WIDTH).max(1) as usize;
    app.set_wall_columns(columns);

    let count = app.favorites.len();
    if count == 0 {
        let hint_area = Rect {
            y: area.y + area.height / 2,
            height: 1.min(area.height),
            ..area
        };
        let hint = Paragraph::new(Span::styled(
            "press / to add a city",
            Style::default().fg(tc.muted),
        ))
        .alignment(Alignment::Center)
        .style(Style::default().bg(tc.bg));
        frame.render_widget(hint, hint_area);
        return;
    }

    let grid_rows = count.div_ceil(columns);
    let panel_height = (area.height / grid_rows as u16).clamp(PANEL_MIN_HEIGHT, PANEL_MAX_HEIGHT);
    let visible_rows = (area.height / panel_height).max(1) as usize;
    let top_pad = if grid_rows <= visible_rows {
        area.height.saturating_sub(grid_rows as u16 * panel_height) / 2
    } else {
        0
    };
    // One empty row stays between the sky and the panels.
    if top_pad >= 2 {
        let band = Rect {
            x: area.x + 1,
            y: area.y,
            width: available,
            height: top_pad - 1,
        };
        draw_sky(frame, app, utc_now, band, tc);
    }

    let base_width = available / columns as u16;
    let wide_columns = (available % columns as u16) as usize;

    let selected_grid_row = app.selected_panel / columns;
    // Scroll only far enough to reveal the selection.
    let top_row = selected_grid_row.saturating_sub(visible_rows.saturating_sub(1));

    let first = top_row * columns;
    for (slot, panel_pos) in (first..count).take(visible_rows * columns).enumerate() {
        let grid_row = (slot / columns) as u16;
        let grid_col = slot % columns;
        let x_offset: u16 = (0..grid_col)
            .map(|c| base_width + u16::from(c < wide_columns))
            .sum();
        let cell_width = base_width + u16::from(grid_col < wide_columns);
        let rect = Rect {
            x: area.x + 1 + x_offset,
            y: area.y + top_pad + grid_row * panel_height,
            width: cell_width.saturating_sub(1),
            height: panel_height,
        };
        if rect.bottom() > area.bottom() || rect.right() > area.right() {
            continue;
        }
        draw_favorite_panel(frame, app, utc_now, panel_pos, rect, tc);
    }
}

/// Fills `band` with the ambient sky for the main clock's daylight, in
/// the subtle colour, with a twinkling star at its peak in the text
/// colour.
fn draw_sky(frame: &mut Frame, app: &App, utc_now: &DateTime<Utc>, band: Rect, tc: &ThemeColors) {
    let is_day = is_daytime_at(
        app.main_clock_place(),
        &utc_now.with_timezone(&app.selection.tz),
    );
    let rows = sky::render(band.width, band.height, utc_now.timestamp(), is_day);
    let lines: Vec<Line> = rows
        .into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|cell| match cell {
                        Some(cell) => Span::styled(
                            cell.glyph.to_string(),
                            Style::default().fg(if cell.bright { tc.fg } else { tc.subtle }),
                        ),
                        None => Span::raw(" "),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(tc.bg)),
        band,
    );
}

fn draw_favorite_panel(
    frame: &mut Frame,
    app: &App,
    utc_now: &DateTime<Utc>,
    panel_pos: usize,
    rect: Rect,
    tc: &ThemeColors,
) {
    let Some(idx) = app.favorites.at(panel_pos) else {
        return;
    };
    let Some(entry) = app.catalogue.get(idx) else {
        return;
    };
    let selected = panel_pos == app.selected_panel;
    let local = utc_now.with_timezone(&entry.tz);

    let border_color = if selected { tc.accent } else { tc.border };
    let title_style = if selected {
        Style::default().fg(tc.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tc.fg)
    };
    let title = format!(
        " {} ",
        truncate_display(entry.city, rect.width.saturating_sub(4) as usize)
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .title_style(title_style)
        .style(Style::default().bg(tc.bg));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let time_style = local_time_style(entry, &local, tc);
    let hero_offset = utc_now
        .with_timezone(&app.selection.tz)
        .offset()
        .fix()
        .local_minus_utc();
    let offset_secs = local.offset().fix().local_minus_utc();
    let diff: Cow<'static, str> = if entry.tz == app.selection.tz {
        Cow::Borrowed("---")
    } else {
        format_diff(offset_secs, hero_offset)
    };
    let mut body = vec![
        Line::from(Span::styled(
            local.format("%H:%M:%S").to_string(),
            time_style,
        )),
        Line::from(vec![
            Span::styled(diff, Style::default().fg(tc.info)),
            Span::styled(
                local.format(" \u{00b7} %a %d %b").to_string(),
                Style::default().fg(tc.muted),
            ),
        ]),
    ];
    body.push(Line::from(vec![
        Span::styled(format_utc_offset(offset_secs), Style::default().fg(tc.info)),
        Span::styled(
            if entry.is_fixed_offset() {
                " \u{00b7} Fixed offset".to_string()
            } else {
                format!(" \u{00b7} {}", entry.tz.name())
            },
            Style::default().fg(tc.muted),
        ),
    ]));
    if let Some(place) = entry.coordinates {
        body.push(sun_line(place, &local, tc));
    }
    // The strip outranks the zone and sun lines, which repeat what it
    // shows, so it takes the last row once the time and offset fit.
    let strip = (inner.width >= STRIP_HOURS && inner.height >= 3)
        .then(|| day_strip(entry.coordinates, entry.tz, app.selection.tz, utc_now, tc));
    body.truncate(usize::from(inner.height) - usize::from(strip.is_some()));
    body.extend(strip);
    frame.render_widget(
        Paragraph::new(body).style(Style::default().bg(tc.bg)),
        inner,
    );

    // Corner marks are the region-scale form of the [ ] marker: the
    // selected panel lights its top-left and bottom-right corners.
    if selected {
        let mark = Style::default().fg(tc.star);
        if let Some(cell) = frame.buffer_mut().cell_mut((rect.x, rect.y)) {
            cell.set_style(mark);
        }
        let (bx, by) = (
            rect.right().saturating_sub(1),
            rect.bottom().saturating_sub(1),
        );
        if let Some(cell) = frame.buffer_mut().cell_mut((bx, by)) {
            cell.set_style(mark);
        }
    }
}

/// One cell for each hour of the main clock's day, coloured by whether
/// the sun is up at `place` halfway through that hour. Every panel
/// samples the same instants, so one column is one moment on the whole
/// wall. The current hour carries the marker colour, in a heavier glyph
/// so that a night or no-daylight cell still shows it.
fn day_strip(
    place: Option<Coordinates>,
    tz: Tz,
    hero_tz: Tz,
    utc_now: &DateTime<Utc>,
    tc: &ThemeColors,
) -> Line<'static> {
    let hero_now = utc_now.with_timezone(&hero_tz);
    let hero_hour = hero_now.hour();
    let hour_start = *utc_now
        - TimeDelta::minutes(i64::from(hero_now.minute()))
        - TimeDelta::seconds(i64::from(hero_now.second()));
    let cells: Vec<Span<'static>> = (0..u32::from(STRIP_HOURS))
        .map(|hour| {
            let midpoint = hour_start
                + TimeDelta::hours(i64::from(hour) - i64::from(hero_hour))
                + TimeDelta::minutes(30);
            let ((glyph, now_glyph), colour) = match place {
                None => (("\u{b7}", "\u{2022}"), tc.fg),
                Some(place) if is_daytime_at_coordinates(place, &midpoint.with_timezone(&tz)) => {
                    (("\u{2588}", "\u{2588}"), tc.good)
                }
                Some(_) => (("\u{2591}", "\u{2593}"), tc.muted),
            };
            if hour == hero_hour {
                Span::styled(now_glyph, Style::default().fg(tc.star))
            } else {
                Span::styled(glyph, Style::default().fg(colour))
            }
        })
        .collect();
    Line::from(cells)
}

/// Bold, in the day colour while the sun is up there and the muted
/// colour while it is down.
fn local_time_style(entry: &TimezoneEntry, local: &DateTime<Tz>, tc: &ThemeColors) -> Style {
    let colour = match entry.coordinates {
        Some(place) if is_daytime_at_coordinates(place, local) => tc.good,
        Some(_) => tc.muted,
        // A fixed offset has no daylight, so neither the day nor the
        // night colour would be true of it.
        None => tc.fg,
    };
    Style::default().fg(colour).add_modifier(Modifier::BOLD)
}

/// The map is about 5 cells wide for each row: 360° by 142° drawn in
/// 2 by 4 Braille dots a cell, with a cell twice as tall as it is wide.
const MAP_COLUMNS_PER_ROW: u16 = 5;
/// The map stops at 58°S and 84°N. South of that is Antarctica, which
/// would take a quarter of the height and holds no city, so a favourite
/// beyond either edge is pinned to it.
const MAP_SOUTH: f64 = -58.0;
const MAP_NORTH: f64 = 84.0;

const FAVOURITE_DOT: &str = "\u{25cf}";
/// The main clock's favourite: a dot inside a ring.
const MAIN_CLOCK_DOT: &str = "\u{25c9}";
/// The main clock's city when it is not a favourite.
const MAIN_CLOCK_RING: &str = "\u{25cb}";

/// Renders the world map view: the night side, the coastline in the day
/// or the night colour, a dot for each favourite with a place, the
/// selected one in the marker colour, a mark for the main clock's city,
/// and a legend of every favourite's local time under it.
///
/// The legend is the strip `h` and `l` walk, so the wall reports one
/// column per favourite and `j`/`k` stay put.
fn draw_map(
    frame: &mut Frame,
    app: &mut App,
    utc_now: &DateTime<Utc>,
    area: Rect,
    tc: &ThemeColors,
) {
    app.set_wall_columns(app.favorites.len());
    let legend = map_legend(app, utc_now, area.width.saturating_sub(2), tc);
    let legend_rows = u16::try_from(legend.len()).unwrap_or(u16::MAX);

    let map_rows = area
        .height
        .saturating_sub(legend_rows.saturating_add(1))
        .min(area.width.saturating_sub(2) / MAP_COLUMNS_PER_ROW);
    let map_width = map_rows * MAP_COLUMNS_PER_ROW;
    let top = area.y
        + area
            .height
            .saturating_sub(map_rows.saturating_add(1).saturating_add(legend_rows))
            / 2;
    let map_area = Rect {
        x: area.x + area.width.saturating_sub(map_width) / 2,
        y: top,
        width: map_width,
        height: map_rows,
    };
    let legend_area = Rect {
        x: area.x + 1,
        y: top.saturating_add(map_rows).saturating_add(1),
        width: area.width.saturating_sub(2),
        height: legend_rows,
    }
    .intersection(area);

    let main_clock = app.selection.catalogue_index;
    let favourite_dots = (0..app.favorites.len()).filter_map(|pos| {
        let idx = app.favorites.at(pos)?;
        let place = app.catalogue.get(idx)?.coordinates?;
        let glyph = if Some(idx) == main_clock {
            MAIN_CLOCK_DOT
        } else {
            FAVOURITE_DOT
        };
        let selected = pos == app.selected_panel;
        Some((
            selected,
            place,
            glyph,
            if selected { tc.star } else { tc.fg },
        ))
    });
    // The ring goes first, so when the main clock is a favourite its own
    // ringed dot, written later in the same cell, covers it.
    let main_clock_ring = main_clock
        .and_then(|idx| app.catalogue.get(idx)?.coordinates)
        .map(|place| {
            (
                false,
                place,
                MAIN_CLOCK_RING,
                clock_colour(app, utc_now, tc),
            )
        });
    let mut marks: Vec<_> = main_clock_ring.into_iter().chain(favourite_dots).collect();
    // The selected dot is written last, so it stays on top of a
    // neighbour in the same cell.
    marks.sort_by_key(|&(selected, ..)| selected);
    let night = night_stipple(utc_now, map_width, map_rows);
    let night_side = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([-180.0, 180.0])
        .y_bounds([MAP_SOUTH, MAP_NORTH])
        .background_color(tc.bg)
        .paint(|ctx| {
            ctx.draw(&Points {
                coords: &night,
                color: tc.border,
            });
        });
    frame.render_widget(night_side, map_area);
    draw_land(frame.buffer_mut(), map_area, utc_now, tc);
    if !map_area.is_empty() {
        for &(_, place, glyph, colour) in &marks {
            let latitude = place.latitude.clamp(MAP_SOUTH, MAP_NORTH);
            if let Some(cell) =
                frame
                    .buffer_mut()
                    .cell_mut(map_dot_cell(map_area, place.longitude, latitude))
            {
                cell.set_symbol(glyph).set_fg(colour);
            }
        }
    }
    frame.render_widget(
        Paragraph::new(legend)
            .alignment(Alignment::Center)
            .style(Style::default().bg(tc.bg)),
        legend_area,
    );
}

/// A canvas paints a shape in one colour, so the land is painted apart
/// and each cell then takes the day or the night colour.
fn draw_land(buffer: &mut Buffer, map_area: Rect, utc_now: &DateTime<Utc>, tc: &ThemeColors) {
    let mut land = Buffer::empty(map_area);
    Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([-180.0, 180.0])
        .y_bounds([MAP_SOUTH, MAP_NORTH])
        .paint(|ctx| {
            ctx.draw(&Map {
                resolution: MapResolution::High,
                color: tc.good,
            });
        })
        .render(map_area, &mut land);
    let utc = utc_now.with_timezone(&Tz::UTC);
    for pos in map_area.positions() {
        let symbol = land[pos].symbol();
        if symbol == " " {
            continue;
        }
        let colour = if is_daytime_at_coordinates(map_cell_place(map_area, pos), &utc) {
            tc.good
        } else {
            tc.accent_secondary
        };
        if let Some(cell) = buffer.cell_mut(pos) {
            cell.set_symbol(symbol).set_fg(colour);
        }
    }
}

fn map_cell_place(map_area: Rect, pos: Position) -> Coordinates {
    let across = f64::from(pos.x - map_area.x) + 0.5;
    let down = f64::from(pos.y - map_area.y) + 0.5;
    Coordinates {
        latitude: MAP_NORTH - down * (MAP_NORTH - MAP_SOUTH) / f64::from(map_area.height),
        longitude: -180.0 + across * 360.0 / f64::from(map_area.width),
    }
}

/// The cell of `map_area` that holds the Braille dot the canvas paints
/// for (longitude, latitude), so a favourite's dot sits on the same
/// coastline cell as its place. The canvas's own `print` places text by
/// cells and truncates, which drifts it up to a cell north-west.
fn map_dot_cell(map_area: Rect, longitude: f64, latitude: f64) -> (u16, u16) {
    let dots_across = f64::from(map_area.width) * 2.0;
    let dots_down = f64::from(map_area.height) * 4.0;
    let dot_x = ((longitude + 180.0) * (dots_across - 1.0) / 360.0).round();
    let dot_y = ((MAP_NORTH - latitude) * (dots_down - 1.0) / (MAP_NORTH - MAP_SOUTH)).round();
    (map_area.x + dot_x as u16 / 2, map_area.y + dot_y as u16 / 4)
}

/// The night side of a map `columns` cells wide and `rows` tall, as
/// (longitude, latitude) points on every other Braille dot across and
/// down, so night reads as a stipple and day stays clear.
///
/// Each point asks [`is_daytime_at_coordinates`] at `utc_now` on the UTC
/// clock, the same sun model the panels and day strips use, so a
/// city's dot and its panel always agree on whether it is night.
fn night_stipple(utc_now: &DateTime<Utc>, columns: u16, rows: u16) -> Vec<(f64, f64)> {
    let utc = utc_now.with_timezone(&Tz::UTC);
    let across = u32::from(columns);
    let down = u32::from(rows) * 2;
    let step_x = 360.0 / f64::from(across);
    let step_y = (MAP_NORTH - MAP_SOUTH) / f64::from(down);
    (0..down)
        .flat_map(|j| (0..across).map(move |i| (i, j)))
        .map(|(i, j)| {
            (
                -180.0 + (f64::from(i) + 0.25) * step_x,
                MAP_SOUTH + (f64::from(j) + 0.25) * step_y,
            )
        })
        .filter(|&(longitude, latitude)| {
            let place = Coordinates {
                latitude,
                longitude,
            };
            !is_daytime_at_coordinates(place, &utc)
        })
        .collect()
}

/// Every favourite as `City HH:MM`, wrapped onto as many lines as
/// `width` needs, so none falls off the edge. The selected city carries
/// the marker colour.
fn map_legend(
    app: &App,
    utc_now: &DateTime<Utc>,
    width: u16,
    tc: &ThemeColors,
) -> Vec<Line<'static>> {
    const SEPARATOR: &str = " \u{b7} ";
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut used = 0;
    for pos in 0..app.favorites.len() {
        let Some(entry) = app.favorites.at(pos).and_then(|idx| app.catalogue.get(idx)) else {
            continue;
        };
        let local = utc_now.with_timezone(&entry.tz);
        let time = local.format("%H:%M").to_string();
        let item_width = entry.city.width() + 1 + time.width();
        if lines.is_empty() || used + SEPARATOR.width() + item_width > usize::from(width) {
            lines.push(Vec::new());
            used = 0;
        }
        let Some(line) = lines.last_mut() else {
            continue;
        };
        if used > 0 {
            line.push(Span::styled(SEPARATOR, Style::default().fg(tc.muted)));
            used += SEPARATOR.width();
        }
        let city_style = if pos == app.selected_panel {
            Style::default().fg(tc.star).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tc.fg)
        };
        line.push(Span::styled(entry.city.to_string(), city_style));
        line.push(Span::raw(" "));
        line.push(Span::styled(time, local_time_style(entry, &local, tc)));
        used += item_width;
    }
    lines.into_iter().map(Line::from).collect()
}

const PICKER_WIDTH: u16 = 56;
const PICKER_HEIGHT: u16 = 16;

/// The add-city modal: query line on top, ranked matches below, the
/// match count cut into the bottom rule. `Enter` adds the marked row
/// to the wall.
fn draw_picker(
    frame: &mut Frame,
    app: &mut App,
    utc_now: &DateTime<Utc>,
    area: Rect,
    tc: &ThemeColors,
) {
    let width = PICKER_WIDTH.min(area.width.saturating_sub(2)).max(20);
    let height = PICKER_HEIGHT.min(area.height.saturating_sub(2)).max(5);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tc.accent))
        .title(" add city ")
        .title_style(Style::default().fg(tc.title))
        .title_bottom(
            Line::from(Span::styled(
                format!(" {} matches ", app.filtered_view.len()),
                Style::default().fg(tc.muted),
            ))
            .right_aligned(),
        )
        .style(Style::default().bg(tc.bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height == 0 {
        return;
    }

    // Query line, with the same horizontal scroll rule the old search
    // bar used: the cursor stays one column inside the right edge.
    let input_width = inner.width.saturating_sub(2);
    let cursor_col = app.search_query[..app.cursor_position].width() as u16;
    let scroll_x = cursor_col.saturating_sub(input_width.saturating_sub(1));
    let query_line: Line = if app.search_query.is_empty() {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(tc.accent)),
            Span::styled(
                "city \u{00b7} state \u{00b7} +5:30",
                Style::default().fg(tc.muted),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(tc.accent)),
            Span::styled(&app.search_query, Style::default().fg(tc.fg)),
        ])
    };
    let input_area = Rect { height: 1, ..inner };
    frame.render_widget(Paragraph::new(query_line).scroll((0, scroll_x)), input_area);
    let visible_col = cursor_col.saturating_sub(scroll_x);
    let max_x = inner.x + inner.width.saturating_sub(1);
    frame.set_cursor_position(((inner.x + 2 + visible_col).min(max_x), inner.y));

    let list_area = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };
    if app.filtered_view.is_empty() {
        let hint = if app.search_query.is_empty() {
            "no cities"
        } else {
            "no matches \u{00b7} try: city \u{00b7} +5:30 \u{00b7} asia"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(tc.muted))),
            list_area,
        );
        return;
    }

    let capacity = list_area.height as usize;
    let start = if app.selected_row < capacity {
        0
    } else {
        app.selected_row + 1 - capacity
    };
    let rows = app.filtered_view.rows();
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(start)
        .take(capacity)
        .map(|(n, row)| {
            picker_row(
                app,
                utc_now,
                n,
                row.catalogue_idx,
                row.display_name,
                list_area.width,
                tc,
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
}

/// One picker row: `[label]` in the marker pair when it is the one
/// the Enter key would take, a plain padded label otherwise, and the
/// city's current time against the right edge.
fn picker_row<'a>(
    app: &App,
    utc_now: &DateTime<Utc>,
    n: usize,
    idx: usize,
    display_name: &'static str,
    width: u16,
    tc: &ThemeColors,
) -> Line<'a> {
    let Some(entry) = app.catalogue.get(idx) else {
        return Line::from("");
    };
    let selected = n == app.selected_row;
    let local = utc_now.with_timezone(&entry.tz);
    let time = local.format("%H:%M").to_string();

    let mut label = display_name.to_string();
    if !entry.admin1.is_empty() {
        label.push_str(", ");
        label.push_str(entry.admin1);
    }
    label.push_str(" \u{00b7} ");
    label.push_str(entry.cc);

    let star = if app.is_favorite(idx) {
        "\u{2605} "
    } else {
        ""
    };
    // Brackets replace padding: every row reserves the two marker
    // cells, so labels do not shift as the mark moves.
    let budget = width.saturating_sub(2 + time.len() as u16 + 1 + star.width() as u16) as usize;
    let label = truncate_display(&label, budget).into_owned();
    let pad = budget.saturating_sub(label.width());

    let (open, close, label_style) = if selected {
        (
            "[",
            "]",
            Style::default().fg(tc.accent).add_modifier(Modifier::BOLD),
        )
    } else {
        (" ", " ", Style::default().fg(tc.fg))
    };
    Line::from(vec![
        Span::styled(open, Style::default().fg(tc.star)),
        Span::styled(star, Style::default().fg(tc.star)),
        Span::styled(label, label_style),
        Span::styled(close, Style::default().fg(tc.star)),
        Span::raw(" ".repeat(pad)),
        Span::styled(format!(" {time}"), Style::default().fg(tc.muted)),
    ])
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect, tc: &ThemeColors) {
    let mode_label = match app.input_mode {
        InputMode::Normal => " NORMAL ",
        InputMode::Search => " SEARCH ",
    };
    let mode_span = Span::styled(
        mode_label,
        Style::default()
            .fg(tc.status_fg)
            .bg(tc.status_bg)
            .add_modifier(Modifier::BOLD),
    );

    // Search-mode status-bar hint:
    //   * Keeps the original Esc/Enter/Ctrl-u bindings users already know.
    //   * Appends a compact `hint:` suffix advertising the three highest-value
    //     query shapes (city · offset · region). Suffix lives at the END so
    //     it's the first thing the existing terminal truncation drops on
    //     narrow screens — the bindings stay visible when space is tight.
    //   * Stays under ~100 chars so it survives terminal widths down to ~80.
    let hints = match app.input_mode {
        InputMode::Normal => " q:quit  /:add  f:remove  J/K:reorder  t:theme  m:map  ?:help",
        InputMode::Search => {
            " Enter:add  Esc:close  Ctrl-u:clear  hint: city \u{00b7} +5:30 \u{00b7} asia"
        }
    };
    let hint_span = Span::styled(hints, Style::default().fg(tc.muted));

    let mut spans = vec![mode_span, hint_span];

    // Show clipboard feedback for ~3s after a copy attempt. The success
    // case keeps the original short "Copied!" label so existing users
    // see the same affordance; the failure case prefixes "✗" and uses a
    // hard red so it stands out against the muted theme palette (no
    // dedicated `bad` slot in `ThemeColors`).
    if let Some(flash) = &app.copy_flash
        && flash.started_at.elapsed() < std::time::Duration::from_secs(3)
    {
        match &flash.status {
            CopyStatus::Success => {
                spans.push(Span::styled(
                    " Copied!",
                    Style::default().fg(tc.good).add_modifier(Modifier::BOLD),
                ));
            }
            CopyStatus::Failure(msg) => {
                spans.push(Span::styled(
                    format!(" \u{2717} Copy failed: {msg}"),
                    Style::default()
                        .fg(Color::Rgb(220, 80, 80))
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
    }

    // Surface startup load warnings / parse errors for the first 10s
    // after launch (the user can also dismiss them with any keypress —
    // see `App::dismiss_startup_messages`). Render them after the copy
    // flash so the most recent feedback wins the rightmost slot.
    if !app.startup_messages.is_empty()
        && app.started_at.elapsed() < std::time::Duration::from_secs(10)
    {
        let joined = app.startup_messages.join("; ");
        spans.push(Span::styled(
            format!(" \u{26A0} {joined}"),
            Style::default()
                .fg(Color::Rgb(220, 80, 80))
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = Line::from(spans);
    let bar = Paragraph::new(line).style(Style::default().bg(tc.bg));
    frame.render_widget(bar, area);
}

// ============================================================================
// Formatting helpers
// ============================================================================

/// Formats the hour difference between two UTC offsets.
///
/// Returns `"0h"` when offsets are identical but timezones differ
/// (e.g. London/Lisbon both at UTC+0 in winter — distinct DST rules,
/// same current offset). The caller is responsible for substituting
/// `"---"` when the row IS the selected timezone.
fn format_diff(offset_secs: i32, selected_offset_secs: i32) -> Cow<'static, str> {
    let diff_secs = offset_secs - selected_offset_secs;
    if diff_secs == 0 {
        return Cow::Borrowed("0h");
    }
    let sign = if diff_secs > 0 { '+' } else { '-' };
    let abs = diff_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    if mins == 0 {
        Cow::Owned(format!("{}{}h", sign, hours))
    } else {
        Cow::Owned(format!("{}{}:{:02}", sign, hours, mins))
    }
}

/// One muted line naming the panel's sun window, with the polar cases
/// spelled out instead of the degenerate clock times they produce.
fn sun_line(place: Coordinates, local: &DateTime<Tz>, tc: &ThemeColors) -> Line<'static> {
    let (sunrise, sunset) = sun_times(place, local);
    let text: Cow<'static, str> = if sunset - sunrise >= 24.0 {
        Cow::Borrowed("sun up all day")
    } else if sunset <= sunrise {
        Cow::Borrowed("sun down all day")
    } else {
        Cow::Owned(format!(
            "rise {} \u{00b7} set {}",
            format_sun_time(sunrise.rem_euclid(24.0)),
            format_sun_time(sunset.rem_euclid(24.0))
        ))
    };
    Line::from(Span::styled(text, Style::default().fg(tc.muted)))
}

/// Formats fractional hours of local clock time as `HH:MM`.
fn format_sun_time(hours: f64) -> String {
    let total_minutes = (hours * 60.0).round() as u32;
    format!("{:02}:{:02}", (total_minutes / 60) % 24, total_minutes % 60)
}

/// Truncates `s` to at most `max_cols` display columns (unicode-width
/// aware), appending an ellipsis when truncated. Wide chars (CJK) and
/// combining marks are accounted for so the result never overflows the
/// requested visual width by more than 1 column (the ellipsis itself).
fn truncate_display(s: &str, max_cols: usize) -> Cow<'_, str> {
    if s.width() <= max_cols {
        return Cow::Borrowed(s);
    }
    // Reserve one column for the ellipsis.
    let budget = max_cols.saturating_sub(1);
    let mut acc = 0usize;
    let mut end = 0usize;
    for (i, ch) in s.char_indices() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if acc + w > budget {
            break;
        }
        acc += w;
        end = i + ch.len_utf8();
    }
    Cow::Owned(format!("{}\u{2026}", &s[..end]))
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::config;

    fn render_buffer(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        render_buffer_at(app, width, height, Utc::now())
    }

    fn render_buffer_at(
        app: &mut App,
        width: u16,
        height: u16,
        now: DateTime<Utc>,
    ) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw_at(frame, app, now)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// Renders the whole frame as one string per buffer row.
    fn render_app(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let buffer = render_buffer(app, width, height);
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// Whether `c` is a block or octant glyph, the characters the hero
    /// art is drawn in. No text or border uses them, but a panel's day
    /// strip draws its day cells as full blocks.
    fn is_art_glyph(c: char) -> bool {
        c != ' ' && ratatui::symbols::pixel::OCTANTS.contains(&c)
    }

    /// Measures the hero art as (rows with art glyphs, widest ink span
    /// in cells), so a test can tell one art scale from another. Rows
    /// with a panel border are skipped, so a day strip is not art.
    fn hero_ink(rows: &[String]) -> (usize, usize) {
        let mut ink_rows = 0;
        let mut min_col = usize::MAX;
        let mut max_col = 0;
        for row in rows.iter().filter(|row| !row.contains('\u{2502}')) {
            let mut inked = false;
            for (col, c) in row.chars().enumerate() {
                if is_art_glyph(c) {
                    inked = true;
                    min_col = min_col.min(col);
                    max_col = max_col.max(col);
                }
            }
            ink_rows += usize::from(inked);
        }
        let span = if ink_rows == 0 {
            0
        } else {
            max_col - min_col + 1
        };
        (ink_rows, span)
    }

    /// A wall app with one favorite per query, added through the
    /// picker flow the user would take.
    fn wall_app(queries: &[&str]) -> App {
        let mut app = App::with_config(config::Config::default());
        for query in queries {
            app.enter_search();
            for c in query.chars() {
                app.search_input(c);
            }
            app.commit_search_result_and_exit();
        }
        app
    }

    fn test_app() -> App {
        App::with_config(config::Config::default())
    }

    #[test]
    fn the_empty_wall_says_how_to_add_a_city() {
        let mut app = test_app();

        let rendered = render_app(&mut app, 80, 24).join("\n");

        assert!(
            rendered.contains("press / to add a city"),
            "got:\n{rendered}"
        );
    }

    /// A wall holding one fixed-offset favorite, keyed the way the
    /// config file stores it.
    fn fixed_offset_app(city: &str) -> App {
        App::with_config(config::Config {
            favorites: vec![config::FavoriteEntry::City {
                city: city.to_string(),
                admin1: String::new(),
                cc: String::new(),
            }],
            ..config::Config::default()
        })
    }

    /// A fixed-offset favorite shows as its offset and nothing else.
    /// Given a wall with the UTC-5 fixed offset,
    /// when the wall renders,
    /// then its panel names UTC-5 as a fixed offset, with no zone name
    /// and no sunrise line.
    /// Why it matters: the zone name inverts the sign, since `Etc/GMT+5`
    /// is UTC-5, and a fixed offset has no place, so it has no sunrise.
    #[test]
    fn a_fixed_offset_panel_shows_the_offset_and_no_zone_name_or_sunrise() {
        // Given:
        let mut app = fixed_offset_app("UTC-5");

        // When:
        let screen = render_app(&mut app, 100, 30).join("\n");

        // Then:
        assert!(screen.contains("UTC-5"), "got:\n{screen}");
        assert!(screen.contains("Fixed offset"), "got:\n{screen}");
        assert!(!screen.contains("Etc/"), "got:\n{screen}");
        assert!(
            !screen.contains("rise ") && !screen.contains("sun up") && !screen.contains("sun down"),
            "got:\n{screen}"
        );
    }

    #[test]
    fn the_wall_shows_a_panel_per_favorite() {
        let mut app = wall_app(&["tokyo", "london"]);

        let rendered = render_app(&mut app, 80, 24).join("\n");

        // The border corner pins each name to its panel title: the
        // clock line above also names the main clock's city.
        assert!(rendered.contains("\u{250c} Tokyo "), "got:\n{rendered}");
        assert!(rendered.contains("\u{250c} London "), "got:\n{rendered}");
        assert!(!rendered.contains("press / to add a city"));
    }

    #[test]
    fn the_selected_panel_lights_its_corners_in_the_marker_colour() {
        let mut app = wall_app(&["tokyo", "london"]);
        app.panel_first();
        let star = app.theme.colors().star;
        let border = app.theme.colors().border;

        let buffer = render_buffer(&mut app, 80, 24);

        let corners: Vec<&ratatui::buffer::Cell> = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .map(|pos| &buffer[pos])
            .filter(|cell| cell.symbol() == "\u{250c}")
            .collect();
        assert_eq!(corners.len(), 2, "one top-left corner per panel");
        assert_eq!(corners[0].fg, star);
        // The unselected panel keeps the plain border.
        assert_eq!(corners[1].fg, border);
    }

    #[test]
    fn the_picker_lists_matches_and_the_count() {
        let mut app = test_app();
        app.enter_search();
        for c in "boston".chars() {
            app.search_input(c);
        }

        let rendered = render_app(&mut app, 80, 24).join("\n");

        assert!(rendered.contains(" add city "), "got:\n{rendered}");
        assert!(
            rendered.contains("Boston, Massachusetts"),
            "got:\n{rendered}"
        );
        assert!(rendered.contains("matches"), "got:\n{rendered}");
    }

    #[test]
    fn the_picker_marks_the_row_enter_would_take() {
        let mut app = test_app();
        app.enter_search();
        for c in "boston".chars() {
            app.search_input(c);
        }
        let tc = app.theme.colors();

        let buffer = render_buffer(&mut app, 80, 24);

        let mut found = false;
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "[" {
                    assert_eq!(
                        cell.fg, tc.star,
                        "the marker pair renders in the marker colour"
                    );
                    let label = &buffer[(x + 1, y)];
                    assert_eq!(
                        label.fg, tc.accent,
                        "the marked label renders in the accent"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "no marker rendered for the selected row");
    }

    #[test]
    fn a_query_that_matches_nothing_suggests_the_search_syntax() {
        let mut app = test_app();
        app.enter_search();
        for c in "zzzzznotacity".chars() {
            app.search_input(c);
        }

        let rendered = render_app(&mut app, 80, 24).join("\n");

        assert!(rendered.contains("no matches"), "got:\n{rendered}");
        assert!(rendered.contains("+5:30"), "got:\n{rendered}");
    }

    #[test]
    fn the_wall_scrolls_to_keep_the_selected_panel_visible() {
        let mut app = wall_app(&["tokyo", "london", "paris", "denver", "cairo", "sydney"]);
        // One column and room for two panel rows force scrolling.
        app.panel_last();

        let rendered = render_app(&mut app, 30, 20).join("\n");

        assert!(rendered.contains(" Sydney "), "got:\n{rendered}");
    }

    fn render_help(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let tc = app.theme.colors();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_help_popup(frame, app, area, &tc);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_help_overlay_says_how_to_dismiss_it_on_a_short_terminal() {
        // 80x24 cannot show the whole overlay at once, so the way out
        // must not be one of the lines that scrolls off.
        let mut app = test_app();

        let rendered = render_help(&mut app, 80, 24).join("\n");

        assert!(rendered.contains("close"), "got:\n{rendered}");
    }

    #[test]
    fn every_help_line_is_reachable_by_scrolling() {
        let mut app = test_app();
        let mut seen = String::new();

        for _ in 0..60 {
            seen.push_str(&render_help(&mut app, 80, 24).join("\n"));
            app.scroll_help(1);
        }

        for expected in [
            "Ctrl-w",
            "Ctrl-k",
            "Ctrl-f",
            "eastern time",
            // The full sentence, so a popup too narrow for it fails here
            // rather than silently clipping the closing bracket.
            "Jump to start / end (also Ctrl-a / Ctrl-e)",
            "CURRENT local time (DST-adjusted).",
        ] {
            assert!(
                seen.contains(expected),
                "{expected} was never rendered in full"
            );
        }
    }

    #[test]
    fn the_help_scroll_stops_at_the_last_line() {
        let mut app = test_app();
        for _ in 0..500 {
            app.scroll_help(1);
        }
        let at_end = render_help(&mut app, 80, 24).join("\n");

        assert!(at_end.contains("Tip:"), "got:\n{at_end}");
    }

    #[test]
    fn the_picker_count_reflects_the_filter() {
        let mut app = test_app();
        app.enter_search();
        for c in "tokyo".chars() {
            app.search_input(c);
        }

        let rendered = render_app(&mut app, 80, 24).join("\n");

        let filtered = app.filtered_view.len();
        assert!(filtered < crate::timezone::all_timezones().len());
        assert!(
            rendered.contains(&format!("{filtered} matches")),
            "got:\n{rendered}"
        );
    }

    #[test]
    fn the_hero_clock_fills_a_tall_terminal_with_full_height_digits() {
        let mut app = wall_app(&["tokyo"]);

        let rows = render_app(&mut app, 100, 40);

        let ink_rows = rows
            .iter()
            .take(13)
            .filter(|row| row.contains('\u{2588}'))
            .count();
        assert!(
            ink_rows >= 6,
            "the hero art spans only {ink_rows} rows:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn the_hero_clock_drops_the_art_on_a_short_terminal() {
        let mut app = wall_app(&["tokyo"]);

        let rows = render_app(&mut app, 80, 12);

        let rendered = rows.join("\n");
        assert_eq!(
            hero_ink(&rows).0,
            0,
            "block art on a 12-row terminal:\n{rendered}"
        );
        // Only the clock line draws a city name followed by a dot.
        assert!(rendered.contains("Tokyo \u{b7} "), "got:\n{rendered}");
    }

    /// A terminal too short for one panel still draws the clock.
    ///
    /// Given: one favourite, so the wall needs a single grid row.
    /// When: the frame is 9 rows tall, which leaves the wall 3 rows,
    /// one fewer than the shortest panel.
    /// Then: the frame draws, and the clock line names the city.
    ///
    /// Why it matters: a split pane or a small window is a normal
    /// place to keep a clock, and a short wall must not stop the frame.
    #[test]
    fn a_wall_shorter_than_one_panel_still_draws_the_clock() {
        // Given:
        let mut app = wall_app(&["tokyo"]);

        // When:
        let rows = render_app(&mut app, 80, 9);

        // Then:
        let rendered = rows.join("\n");
        assert!(rendered.contains("Tokyo \u{b7} "), "got:\n{rendered}");
    }

    /// A narrow terminal gets a smaller clock, and only the narrowest
    /// falls back to text.
    ///
    /// Given: a terminal tall enough for the medium clock.
    /// When: it is drawn at 66, 64 and 30 columns.
    /// Then: 64 columns draw fewer art rows than 66 but still some art,
    /// and 30 columns draw no art.
    ///
    /// Why it matters: the clock is the main thing the app shows, so a
    /// terminal two columns short of one size should get the next size
    /// down, not lose the art altogether.
    #[test]
    fn a_narrower_terminal_steps_the_clock_down_a_size_before_text() {
        // Given:
        let mut app = wall_app(&["tokyo"]);

        // When:
        let (rows_at_66, _) = hero_ink(&render_app(&mut app, 66, 24));
        let narrow = render_app(&mut app, 64, 24);
        let (rows_at_64, _) = hero_ink(&narrow);
        let (rows_at_30, _) = hero_ink(&render_app(&mut app, 30, 24));

        // Then:
        assert!(
            rows_at_66 > rows_at_64 && rows_at_64 > 0,
            "66 columns: {rows_at_66} art rows, 64 columns: {rows_at_64}:\n{}",
            narrow.join("\n")
        );
        assert_eq!(rows_at_30, 0, "art on a 30-column terminal");
    }

    /// The hero digits draw their curves in octant cells.
    ///
    /// Given: a terminal that holds the medium clock.
    /// When: the frame is drawn.
    /// Then: the art uses a glyph finer than a half block.
    ///
    /// Why it matters: half blocks give one dot across a cell, so a
    /// curve steps in whole cells. Octants give two across and four
    /// down, which is what makes the curves smooth.
    #[test]
    fn the_hero_digits_draw_their_curves_in_octant_cells() {
        // Given:
        let mut app = wall_app(&["tokyo"]);

        // When:
        let rendered = render_app(&mut app, 100, 40).join("\n");

        // Then:
        let half_blocks = [' ', '\u{2588}', '\u{2580}', '\u{2584}'];
        assert!(
            rendered
                .chars()
                .any(|c| is_art_glyph(c) && !half_blocks.contains(&c)),
            "no glyph finer than a half block:\n{rendered}"
        );
    }

    #[test]
    fn the_hero_digits_double_on_a_large_terminal() {
        let mut app = wall_app(&["tokyo"]);

        let rows = render_app(&mut app, 140, 30);

        let (ink_rows, span) = hero_ink(&rows);
        assert!(
            ink_rows >= 12,
            "the art spans only {ink_rows} rows:\n{}",
            rows.join("\n")
        );
        assert!(
            span >= 100,
            "the art spans only {span} cells:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn the_hero_digits_triple_on_a_huge_terminal() {
        let mut app = wall_app(&["tokyo"]);

        let rows = render_app(&mut app, 200, 48);

        let (ink_rows, span) = hero_ink(&rows);
        assert!(
            ink_rows >= 18,
            "the art spans only {ink_rows} rows:\n{}",
            rows.join("\n")
        );
        assert!(
            span >= 150,
            "the art spans only {span} cells:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn the_wall_panels_stretch_to_the_right_edge() {
        let mut app = wall_app(&["tokyo", "london", "paris", "denver"]);

        let buffer = render_buffer(&mut app, 80, 24);

        // A stretched grid puts the rightmost panel border one gutter
        // cell in from the frame edge, mirroring the left margin.
        let found = (0..buffer.area.height).any(|y| buffer[(78, y)].symbol() == "\u{2510}");
        assert!(found, "no panel corner lands beside the right edge");
    }

    fn is_day_cell(c: char) -> bool {
        c == '\u{2588}'
    }

    fn is_night_cell(c: char) -> bool {
        matches!(c, '\u{2591}' | '\u{2593}')
    }

    /// Finds each panel's day strip as (row, first column, cells), in
    /// reading order. A strip is 24 day, night or no-daylight cells
    /// right after a panel's left border.
    fn day_strips(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16, Vec<char>)> {
        let is_cell =
            |c: char| is_day_cell(c) || is_night_cell(c) || matches!(c, '\u{b7}' | '\u{2022}');
        let mut strips = Vec::new();
        for y in 0..buffer.area.height {
            let row: Vec<char> = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect();
            for x in 0..row.len().saturating_sub(24) {
                if row[x] == '\u{2502}' && row[x + 1..x + 25].iter().all(|&c| is_cell(c)) {
                    strips.push((y, x as u16 + 1, row[x + 1..x + 25].to_vec()));
                }
            }
        }
        strips
    }

    fn midsummer_noon_utc() -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 6, 21, 12, 0, 0).unwrap()
    }

    /// Each panel shows its city's daylight on the main clock's hours.
    ///
    /// Given: London and Tokyo on the wall, Tokyo on the main clock, at
    /// noon UTC on 21 June, which is 21:00 in Tokyo and 13:00 in London.
    /// When: the wall is drawn with room for the strips.
    /// Then: column 21 (13:30 in London) is day in London and night in
    /// Tokyo, column 8 (00:30 in London) is night in London and day in
    /// Tokyo, and column 21 carries the marker colour in both strips.
    ///
    /// Why it matters: the strip answers when everyone is awake, which
    /// only works if one column is one instant in every panel and the
    /// columns read in the time the user is thinking in.
    #[test]
    fn each_panel_shows_its_daylight_on_the_main_clocks_hours() {
        // Given:
        let mut app = wall_app(&["london", "tokyo"]);
        assert_eq!(app.selection.city_name, "Tokyo");

        // When:
        let buffer = render_buffer_at(&mut app, 80, 40, midsummer_noon_utc());

        // Then:
        let strips = day_strips(&buffer);
        assert_eq!(strips.len(), 2, "strips: {strips:?}");
        let (_, _, london) = &strips[0];
        let (_, _, tokyo) = &strips[1];
        assert!(
            is_day_cell(london[21]) && is_night_cell(london[8]),
            "London: {london:?}"
        );
        assert!(
            is_night_cell(tokyo[21]) && is_day_cell(tokyo[8]),
            "Tokyo: {tokyo:?}"
        );
        let marker = app.theme.colors().star;
        for (y, x, _) in &strips {
            assert_eq!(buffer[(x + 21, *y)].fg, marker, "now cell at row {y}");
        }
    }

    /// A fixed-offset strip shows no day and no night.
    ///
    /// Given: the UTC-5 fixed offset on the wall.
    /// When: the wall is drawn with room for the strip.
    /// Then: its strip has no day cell and no night cell.
    ///
    /// Why it matters: a fixed offset has no place, so it has no
    /// sunrise, and a day colour would show daylight that does not
    /// exist.
    #[test]
    fn a_fixed_offset_strip_shows_no_day_and_no_night() {
        // Given:
        let mut app = fixed_offset_app("UTC-5");

        // When:
        let buffer = render_buffer_at(&mut app, 80, 40, midsummer_noon_utc());

        // Then:
        let strips = day_strips(&buffer);
        assert_eq!(strips.len(), 1, "strips: {strips:?}");
        assert!(
            !strips[0]
                .2
                .iter()
                .any(|&c| is_day_cell(c) || is_night_cell(c)),
            "cells: {:?}",
            strips[0].2
        );
    }

    /// A short panel keeps its strip and drops the zone name first.
    ///
    /// Given: four favourites on a terminal that leaves each panel
    /// three lines inside.
    /// When: the wall is drawn.
    /// Then: every panel shows a strip, and no panel shows its zone
    /// name.
    ///
    /// Why it matters: the zone name and the sun times repeat what the
    /// strip shows, so the strip is the line worth keeping when a panel
    /// cannot hold them all.
    #[test]
    fn a_short_panel_keeps_its_strip_and_drops_the_zone_name() {
        // Given:
        let mut app = wall_app(&["tokyo", "london", "paris", "denver"]);

        // When:
        let buffer = render_buffer_at(&mut app, 80, 23, midsummer_noon_utc());

        // Then:
        let strips = day_strips(&buffer);
        let rendered: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert_eq!(strips.len(), 4, "got:\n{rendered}");
        assert!(!rendered.contains("Asia/Tokyo"), "got:\n{rendered}");
    }

    fn favorites_app(cities: &[(&str, &str, &str)]) -> App {
        App::with_config(config::Config {
            favorites: cities
                .iter()
                .map(|&(city, admin1, cc)| config::FavoriteEntry::City {
                    city: city.to_string(),
                    admin1: admin1.to_string(),
                    cc: cc.to_string(),
                })
                .collect(),
            ..config::Config::default()
        })
    }

    /// Every map dot as (column, row, colour), in reading order.
    fn map_dots(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16, Color)> {
        map_marks(buffer, FAVOURITE_DOT)
    }

    /// Every cell drawn as `glyph`, as (column, row, colour), in reading
    /// order.
    fn map_marks(buffer: &ratatui::buffer::Buffer, glyph: &str) -> Vec<(u16, u16, Color)> {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|&pos| buffer[pos].symbol() == glyph)
            .map(|(x, y)| (x, y, buffer[(x, y)].fg))
            .collect()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    /// The map view puts a dot where each favourite is, and lists every
    /// favourite with its time.
    ///
    /// Given: London (selected), Tokyo, Sydney and the UTC-5 fixed
    /// offset, at noon UTC on 21 June, in the map view.
    /// When: the frame is drawn.
    /// Then: there is no panel, there are three dots, Tokyo's is east of
    /// London's and north of Sydney's, London's carries the marker
    /// colour, and the legend gives every favourite's local time.
    ///
    /// Why it matters: a dot in the wrong place, or a favourite missing
    /// from the legend, shows the user a world that is not theirs. A
    /// fixed offset has no place, so it has no dot, but it still has a
    /// time.
    #[test]
    fn the_map_view_puts_each_placed_favourite_where_it_is() {
        // Given:
        let mut app = favorites_app(&[
            ("London", "England", "GB"),
            ("Tokyo", "Tokyo", "JP"),
            ("Sydney", "New South Wales", "AU"),
            ("UTC-5", "", ""),
        ]);
        app.toggle_map();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        let text = buffer_text(&buffer);
        assert!(
            !text.contains('\u{250c}'),
            "a panel in the map view:\n{text}"
        );
        let dots = map_dots(&buffer);
        assert_eq!(dots.len(), 3, "dots: {dots:?}\n{text}");
        let star = app.theme.colors().star;
        let london: Vec<_> = dots.iter().filter(|d| d.2 == star).collect();
        let rest: Vec<_> = dots.iter().filter(|d| d.2 != star).collect();
        assert_eq!(london.len(), 1, "one marked dot: {dots:?}");
        let (tokyo, sydney) = (rest[0], rest[1]);
        assert!(
            london[0].0 < tokyo.0 && tokyo.1 < sydney.1 && london[0].1 < sydney.1,
            "London {:?}, Tokyo {tokyo:?}, Sydney {sydney:?}",
            london[0]
        );
        for entry in ["London 13:00", "Tokyo 21:00", "Sydney 22:00", "UTC-5 07:00"] {
            assert!(text.contains(entry), "no {entry:?} in:\n{text}");
        }
    }

    /// A favourite south of the map's crop stays on the map.
    ///
    /// Given: Casey, an Antarctic base at 66°S, in the map view.
    /// When: the frame is drawn.
    /// Then: its dot is on the map's bottom row.
    ///
    /// Why it matters: the map stops at 58°S to keep Antarctica from
    /// filling a quarter of it, and every Antarctic zone is in the
    /// catalogue, so a dot outside the crop would vanish.
    /// At an equinox noon UTC the night side is the half of the world
    /// around the date line.
    ///
    /// Given: 20 March 2026 at 12:00 UTC, when the sun is overhead near
    /// 0° longitude.
    /// When: the night stipple is laid out for a map.
    /// Then: every night point lies more than 80° from the prime
    /// meridian, and some lie within 10° of the date line.
    ///
    /// Why it matters: the shading is the map's answer to "where is it
    /// night now", so its terminator must sit where the sun puts it.
    #[test]
    fn the_night_side_at_an_equinox_noon_is_around_the_date_line() {
        // Given:
        use chrono::TimeZone;
        let equinox_noon = Utc.with_ymd_and_hms(2026, 3, 20, 12, 0, 0).unwrap();

        // When:
        let night = night_stipple(&equinox_noon, 60, 12);

        // Then:
        assert!(
            night.iter().all(|&(longitude, _)| longitude.abs() > 80.0),
            "night near the prime meridian: {night:?}"
        );
        assert!(
            night.iter().any(|&(longitude, _)| longitude.abs() > 170.0),
            "no night at the date line: {night:?}"
        );
    }

    /// At midsummer midnight UTC the far north is still in daylight.
    ///
    /// Given: 21 June 2026 at 00:00 UTC.
    /// When: the night stipple is laid out for a map.
    /// Then: no night point lies north of 70°, and night does reach
    /// the far south.
    ///
    /// Why it matters: the midnight sun is the case a plain "dark side
    /// of the globe" drawing gets wrong.
    #[test]
    fn the_arctic_has_no_night_at_midsummer_midnight() {
        // Given:
        use chrono::TimeZone;
        let midsummer_midnight = Utc.with_ymd_and_hms(2026, 6, 21, 0, 0, 0).unwrap();

        // When:
        let night = night_stipple(&midsummer_midnight, 60, 12);

        // Then:
        assert!(
            night.iter().all(|&(_, latitude)| latitude < 70.0),
            "night in the Arctic: {night:?}"
        );
        assert!(
            night.iter().any(|&(_, latitude)| latitude < -40.0),
            "no night in the far south: {night:?}"
        );
    }

    /// A favourite's dot sits in the cell where the canvas draws its
    /// place.
    ///
    /// Given: cities spread over the map, north and south, east and
    /// west, at three map sizes, and the cell where ratatui's canvas
    /// paints a Braille dot for each city's coordinates.
    /// When: the map dot's cell is computed for the same coordinates.
    /// Then: it is that same cell.
    ///
    /// Why it matters: the coastline is drawn in Braille dots, so a
    /// city's dot must follow the same placement. A text label placed
    /// by cells drifted up to a cell north-west of its coast, which
    /// put Sydney off Australia.
    #[test]
    fn a_favourites_dot_sits_where_the_canvas_draws_its_place() {
        use ratatui::widgets::canvas::Points;
        let places = [
            ("London", -0.13, 51.51),
            ("Sydney", 151.21, -33.87),
            ("Tokyo", 139.69, 35.69),
            ("Buenos Aires", -58.38, -34.60),
            ("Nairobi", 36.82, -1.29),
            ("Tabwakea Village", -157.49, 2.02),
        ];
        for (columns, rows) in [(95_u16, 19_u16), (60, 12), (40, 8)] {
            let area = Rect::new(0, 0, columns, rows);
            for (name, longitude, latitude) in places {
                // Given:
                let mut terminal = Terminal::new(TestBackend::new(columns, rows)).unwrap();
                terminal
                    .draw(|frame| {
                        let canvas = Canvas::default()
                            .marker(Marker::Braille)
                            .x_bounds([-180.0, 180.0])
                            .y_bounds([MAP_SOUTH, MAP_NORTH])
                            .paint(|ctx| {
                                ctx.draw(&Points {
                                    coords: &[(longitude, latitude)],
                                    color: Color::White,
                                });
                            });
                        frame.render_widget(canvas, area);
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let painted: Vec<(u16, u16)> = (0..rows)
                    .flat_map(|y| (0..columns).map(move |x| (x, y)))
                    .filter(|&pos| buffer[pos].symbol() != " ")
                    .collect();
                assert_eq!(painted.len(), 1, "{name}: {painted:?}");

                // When:
                let dot = map_dot_cell(area, longitude, latitude);

                // Then:
                assert_eq!(dot, painted[0], "{name} on a {columns}x{rows} map");
            }
        }
    }

    #[test]
    fn an_antarctic_favourite_is_pinned_to_the_maps_edge() {
        // Given:
        let mut app = favorites_app(&[("Casey", "", "AQ"), ("London", "England", "GB")]);
        app.toggle_map();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        let dots = map_dots(&buffer);
        assert_eq!(dots.len(), 2, "dots: {dots:?}\n{}", buffer_text(&buffer));
        let lowest = dots.iter().map(|d| d.1).max().unwrap();
        let map_bottom = (0..buffer.area.height)
            .filter(|&y| {
                (0..buffer.area.width).any(|x| {
                    ('\u{2801}'..='\u{28ff}')
                        .contains(&buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                })
            })
            .max()
            .unwrap();
        assert_eq!(lowest, map_bottom, "Casey is not on the bottom row");
    }

    /// On a narrow terminal the legend wraps, so every favourite stays
    /// on screen.
    ///
    /// Given: six favourites in the map view on a 60-column terminal.
    /// When: the frame is drawn.
    /// Then: every city name appears.
    ///
    /// Why it matters: `h` and `l` move the selection along the legend,
    /// and a selected favourite cut off the right edge could not be seen.
    #[test]
    fn the_map_legend_wraps_so_every_favourite_stays_on_screen() {
        // Given:
        let cities = [
            ("London", "England", "GB"),
            ("Tokyo", "Tokyo", "JP"),
            ("Sydney", "New South Wales", "AU"),
            ("Paris", "Île-de-France", "FR"),
            ("Denver", "Colorado", "US"),
            ("Nairobi", "Nairobi County", "KE"),
        ];
        let mut app = favorites_app(&cities);
        app.toggle_map();

        // When:
        let buffer = render_buffer_at(&mut app, 60, 30, midsummer_noon_utc());

        // Then:
        let text = buffer_text(&buffer);
        for (city, _, _) in cities {
            assert!(text.contains(city), "no {city} in:\n{text}");
        }
    }

    /// The rows of a frame that hold the sky: any row with a Braille
    /// glyph, since in the panel view nothing else draws in Braille.
    fn sky_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect::<String>()
            })
            .filter(|row| row.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)))
            .collect()
    }

    /// A night sky fills the band between the clock and the panels.
    ///
    /// Given: Tokyo on the main clock at noon UTC on 21 June, which is
    /// 21:00 in Tokyo, on a terminal that leaves a band above the panels.
    /// When: the panel view is drawn.
    /// Then: the band has stars and no cloud.
    ///
    /// Why it matters: the sky follows the main clock's daylight, so a
    /// cloud at night would contradict the clock above it.
    #[test]
    fn a_night_sky_fills_the_band_above_the_panels() {
        // Given:
        let mut app = wall_app(&["tokyo"]);

        // When:
        let rows = sky_rows(&render_buffer_at(&mut app, 100, 48, midsummer_noon_utc()));

        // Then:
        assert!(!rows.is_empty(), "no sky");
        assert!(
            rows.iter().all(|row| !row.contains('\u{28e4}')),
            "a cloud at night:\n{}",
            rows.join("\n")
        );
    }

    /// The main clock takes its daylight from its own city, not from the
    /// largest city of its zone.
    ///
    /// Given: Lhasa on the main clock at noon UTC on 21 June, 20:00 on
    /// the Asia/Shanghai clock. The sun has set in Shanghai, far to the
    /// east, and not yet in Lhasa.
    /// When: the panel view is drawn, then the map.
    /// Then: the sky has clouds, and the main clock's ring on the map
    /// is in the day colour.
    ///
    /// Why it matters: a panel for Lhasa shows day, so a night sky and a
    /// night clock above it would contradict the wall.
    #[test]
    fn the_main_clock_takes_daylight_from_its_own_city() {
        // Given:
        let mut app = favorites_app(&[("London", "England", "GB")]);
        app.enter_search();
        for c in "lhasa".chars() {
            app.search_input(c);
        }
        app.select_timezone();
        app.exit_search();
        assert_eq!(app.selection.city_name, "Lhasa");
        let tc = app.theme.colors();

        // When:
        let sky = sky_rows(&render_buffer_at(&mut app, 100, 48, midsummer_noon_utc()));
        app.toggle_map();
        let map = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        assert!(
            sky.iter().any(|row| row.contains('\u{28e4}')),
            "no cloud by day:\n{}",
            sky.join("\n")
        );
        let ring_colours: Vec<Color> = map_marks(&map, MAIN_CLOCK_RING)
            .into_iter()
            .map(|(.., colour)| colour)
            .collect();
        assert_eq!(ring_colours, vec![tc.accent]);
    }

    /// Clouds drift one cell to the right every four seconds.
    ///
    /// Given: London on the main clock at noon UTC on 21 June, 13:00
    /// in London.
    /// When: the panel view is drawn, then drawn again four seconds
    /// later.
    /// Then: the band has clouds, and the later band is the earlier one
    /// moved one cell right, wrapping at the edge.
    ///
    /// Why it matters: the drift is the sky's only motion by day, and
    /// it must be the same clouds moving, not new ones each frame.
    #[test]
    fn day_clouds_drift_one_cell_every_four_seconds() {
        // Given:
        let mut app = wall_app(&["london"]);
        let noon = midsummer_noon_utc();

        // When:
        let before = sky_rows(&render_buffer_at(&mut app, 100, 48, noon));
        let after = sky_rows(&render_buffer_at(
            &mut app,
            100,
            48,
            noon + TimeDelta::seconds(4),
        ));

        // Then:
        assert!(
            before.iter().any(|row| row.contains('\u{28e4}')),
            "no cloud by day:\n{}",
            before.join("\n")
        );
        assert_eq!(before.len(), after.len());
        for (earlier, later) in before.iter().zip(&after) {
            // Column 0 is the wall's margin, outside the sky.
            let sky: Vec<char> = earlier.chars().skip(1).collect();
            let mut moved = sky.clone();
            moved.rotate_right(1);
            let later: Vec<char> = later.chars().skip(1).collect();
            assert_eq!(later, moved, "not a one-cell drift");
        }
    }

    /// Stars twinkle on a two-second beat.
    ///
    /// Given: Tokyo on the main clock at night.
    /// When: the panel view is drawn at one second, the next, and the
    /// one after.
    /// Then: the sky changes after one second and returns after two.
    ///
    /// Why it matters: the sky moves only on the one-second tick the
    /// app already has, so it costs no extra frames, and a star that
    /// returned at random would read as noise.
    #[test]
    fn stars_twinkle_on_a_two_second_beat() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let night = midsummer_noon_utc();

        // When:
        let mut at = |seconds| {
            sky_rows(&render_buffer_at(
                &mut app,
                100,
                48,
                night + TimeDelta::seconds(seconds),
            ))
        };
        let (first, second, third) = (at(0), at(1), at(2));

        // Then:
        assert_ne!(first, second, "no twinkle after one second");
        assert_eq!(first, third, "the sky did not return after two seconds");
    }

    #[test]
    fn a_tall_panel_shows_the_zone_and_the_sun_window() {
        let mut app = wall_app(&["tokyo"]);

        let rendered = render_app(&mut app, 80, 30).join("\n");

        assert!(rendered.contains("Asia/Tokyo"), "got:\n{rendered}");
        assert!(rendered.contains("rise "), "got:\n{rendered}");
        assert!(rendered.contains("set "), "got:\n{rendered}");
    }

    /// A binding can reach the overlay and never reach the README, which
    /// is what happened to Ctrl-w, Ctrl-k and Ctrl-f. The README wraps
    /// every key in backticks, so the check can be exact.
    #[test]
    fn every_help_binding_appears_in_the_readme() {
        let readme = include_str!("../README.md");
        let mut missing = Vec::new();

        for &(keys, desc) in NORMAL_MODE_HELP.iter().chain(SEARCH_MODE_HELP) {
            let named = keys.split(" / ").flat_map(str::split_whitespace);
            // Ctrl-a and Ctrl-e are named in a description, not a key column.
            let in_prose = desc
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .filter(|token| token.starts_with("Ctrl-"));

            for token in named.chain(in_prose) {
                if !readme.contains(&format!("`{token}`")) {
                    missing.push(token);
                }
            }
        }

        assert!(
            missing.is_empty(),
            "the overlay names these keys, the README does not: {missing:?}"
        );
    }

    /// The colour of every Braille cell in a frame, with its glyph.
    fn braille_cells(buffer: &ratatui::buffer::Buffer) -> Vec<(char, Color)> {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter_map(|pos| {
                let glyph = buffer[pos].symbol().chars().next()?;
                ('\u{2801}'..='\u{28ff}')
                    .contains(&glyph)
                    .then_some((glyph, buffer[pos].fg))
            })
            .collect()
    }

    /// The map draws land on the day side in the day colour, land on the
    /// night side in the night colour, and a plain favourite's dot in
    /// the text colour.
    ///
    /// Given: London (selected) and Tokyo at noon UTC on 21 June, which
    /// is 13:00 in London and 21:00 in Tokyo, in the map view.
    /// When: the frame is drawn.
    /// Then: the land around London's dot is in the day colour, the land
    /// around Tokyo's dot is in the night colour, Tokyo's dot is in the
    /// text colour, and every other Braille cell is the dim night stipple.
    ///
    /// Why it matters: the land colour shows where it is day, with the
    /// same colours as the day strip and the clock. A plain dot in the
    /// day colour would vanish on day land.
    #[test]
    fn the_map_colours_land_by_day_and_night() {
        // Given:
        let mut app = favorites_app(&[("London", "England", "GB"), ("Tokyo", "Tokyo", "JP")]);
        app.toggle_map();
        let tc = app.theme.colors();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        let dots = map_dots(&buffer);
        let london = dots.iter().find(|d| d.2 == tc.star).copied();
        let tokyo = dots.iter().find(|d| d.2 != tc.star).copied();
        let (Some(london), Some(tokyo)) = (london, tokyo) else {
            panic!("dots: {dots:?}");
        };
        assert_eq!(tokyo.2, tc.fg, "Tokyo's dot");
        let land_around = |(x, y, _): (u16, u16, Color)| -> Vec<Color> {
            (y.saturating_sub(1)..=y + 1)
                .flat_map(|row| (x.saturating_sub(2)..=x + 2).map(move |column| (column, row)))
                .filter_map(|pos| {
                    let glyph = buffer[pos].symbol().chars().next()?;
                    let colour = buffer[pos].fg;
                    (('\u{2801}'..='\u{28ff}').contains(&glyph) && colour != tc.border)
                        .then_some(colour)
                })
                .collect()
        };
        let day_land = land_around(london);
        assert!(!day_land.is_empty(), "no land around London");
        assert!(
            day_land.iter().all(|&c| c == tc.good),
            "London: {day_land:?}"
        );
        let night_land = land_around(tokyo);
        assert!(!night_land.is_empty(), "no land around Tokyo");
        assert!(
            night_land.iter().all(|&c| c == tc.accent_secondary),
            "Tokyo: {night_land:?}"
        );
        let cells = braille_cells(&buffer);
        assert!(
            cells
                .iter()
                .all(|&(_, c)| c == tc.good || c == tc.accent_secondary || c == tc.border),
            "a map cell in another colour: {cells:?}"
        );
    }

    /// The night sky draws its stars in the subtle colour, and a
    /// twinkling star at its peak in the text colour.
    ///
    /// Given: Tokyo on the main clock at night.
    /// When: the panel view is drawn at one second and the next.
    /// Then: every peak glyph is in the text colour, every other star is
    /// subtle, and a peak glyph is drawn at least once.
    ///
    /// Why it matters: a sky in a dim colour is close to invisible, and
    /// the twinkle reads only when the peak is a step brighter than the
    /// other stars.
    #[test]
    fn night_stars_are_subtle_and_a_twinkle_peaks_in_text_colour() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let tc = app.theme.colors();
        let night = midsummer_noon_utc();

        // When:
        let cells: Vec<(char, Color)> = [night, night + TimeDelta::seconds(1)]
            .into_iter()
            .flat_map(|now| braille_cells(&render_buffer_at(&mut app, 100, 48, now)))
            .collect();

        // Then:
        let peak = '\u{283f}';
        assert!(
            cells.iter().any(|&(glyph, _)| glyph == peak),
            "no twinkle peak"
        );
        for (glyph, colour) in cells {
            let expected = if glyph == peak { tc.fg } else { tc.subtle };
            assert_eq!(colour, expected, "star {glyph:?}");
        }
    }

    /// Every star lights at least two Braille dots.
    ///
    /// Given: Tokyo on the main clock at night.
    /// When: the panel view is drawn at one second and the next.
    /// Then: every star glyph has two or more of its eight dots raised.
    ///
    /// Why it matters: a one-dot glyph is about two pixels wide at a
    /// normal font size, so no colour makes it easy to see.
    #[test]
    fn every_star_lights_at_least_two_dots() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let night = midsummer_noon_utc();

        // When:
        let cells: Vec<(char, Color)> = [night, night + TimeDelta::seconds(1)]
            .into_iter()
            .flat_map(|now| braille_cells(&render_buffer_at(&mut app, 100, 48, now)))
            .collect();

        // Then:
        assert!(!cells.is_empty(), "no sky");
        let faint: Vec<char> = cells
            .iter()
            .map(|&(glyph, _)| glyph)
            .filter(|&glyph| (u32::from(glyph) - 0x2800).count_ones() < 2)
            .collect();
        assert!(faint.is_empty(), "one-dot stars: {faint:?}");
    }

    /// A main clock city that is not a favourite gets a ring where it is,
    /// in the big clock's colour.
    ///
    /// Given: London as the only favourite, and Tokyo picked as the main
    /// clock without adding it, at noon UTC on 21 June, which is 21:00
    /// in Tokyo.
    /// When: the map view is drawn.
    /// Then: there is one ring, in the cell where Tokyo's dot is when
    /// Tokyo is a favourite, in the big clock's night colour.
    ///
    /// Why it matters: the map shows the favourites, and without the
    /// ring the city of the big clock above it is nowhere on it.
    #[test]
    fn the_main_clock_city_gets_a_ring_when_it_is_not_a_favourite() {
        // Given:
        let mut favourite = favorites_app(&[("London", "England", "GB"), ("Tokyo", "Tokyo", "JP")]);
        favourite.toggle_map();
        let tokyo_dot = map_dots(&render_buffer_at(
            &mut favourite,
            100,
            40,
            midsummer_noon_utc(),
        ))
        .into_iter()
        .max_by_key(|&(x, _, _)| x);
        let mut app = favorites_app(&[("London", "England", "GB")]);
        app.enter_search();
        for c in "tokyo".chars() {
            app.search_input(c);
        }
        app.select_timezone();
        app.exit_search();
        app.toggle_map();
        let tc = app.theme.colors();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        let rings = map_marks(&buffer, MAIN_CLOCK_RING);
        let Some((x, y, _)) = tokyo_dot else {
            panic!("no Tokyo dot");
        };
        assert_eq!(rings, vec![(x, y, tc.accent_secondary)], "rings");
        assert!(map_marks(&buffer, MAIN_CLOCK_DOT).is_empty());
    }

    /// The main clock's favourite gets a ringed dot in its usual colour.
    ///
    /// Given: London and Tokyo as favourites, Tokyo on the main clock,
    /// and London's panel selected.
    /// When: the map view is drawn.
    /// Then: Tokyo is a ringed dot in the text colour, London is a plain
    /// dot in the marker colour, and there is no ring.
    ///
    /// Why it matters: every city added from search becomes the main
    /// clock, so this is the usual case, and a second mark in the same
    /// cell would hide the favourite's dot.
    #[test]
    fn the_main_clock_favourite_gets_a_ringed_dot() {
        // Given:
        let mut app = wall_app(&["london", "tokyo"]);
        app.panel_first();
        app.toggle_map();
        let tc = app.theme.colors();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        let ringed = map_marks(&buffer, MAIN_CLOCK_DOT);
        let plain = map_dots(&buffer);
        assert_eq!(ringed.len(), 1, "ringed dots: {ringed:?}");
        assert_eq!(plain.len(), 1, "plain dots: {plain:?}");
        assert!(ringed[0].0 > plain[0].0, "Tokyo is not east of London");
        assert_eq!(ringed[0].2, tc.fg, "Tokyo's ringed dot");
        assert_eq!(plain[0].2, tc.star, "London's dot");
        assert!(map_marks(&buffer, MAIN_CLOCK_RING).is_empty());
    }

    /// A main clock with no place has no mark on the map.
    ///
    /// Given: London as a favourite and UTC, the startup main clock.
    /// When: the map view is drawn.
    /// Then: there is no ring and no ringed dot.
    ///
    /// Why it matters: UTC and a fixed offset are not places, so a mark
    /// anywhere on the map would be wrong.
    #[test]
    fn a_main_clock_with_no_place_has_no_mark_on_the_map() {
        // Given:
        let mut app = favorites_app(&[("London", "England", "GB")]);
        app.toggle_map();

        // When:
        let buffer = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        assert!(map_marks(&buffer, MAIN_CLOCK_RING).is_empty());
        assert!(map_marks(&buffer, MAIN_CLOCK_DOT).is_empty());
        assert_eq!(map_dots(&buffer).len(), 1);
    }
}
