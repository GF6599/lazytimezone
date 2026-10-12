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
    layout::Alignment,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::app::{App, CopyStatus, InputMode};
use crate::digits;
use crate::theme::ThemeColors;
use crate::timezone::{
    Coordinates, format_utc_offset, is_daytime_at, is_daytime_at_coordinates, sun_times,
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
    draw_wall(frame, app, &now, outer[2], &tc);
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

/// Renders the hero clock at the size [`hero_variant`] chose.
///
/// ## Day/night colouring
///
/// Daytime uses `accent` (bright), nighttime uses `accent_secondary`
/// (dim). The day/night decision delegates to
/// [`is_daytime_at`](crate::timezone::is_daytime_at), which computes
/// sunrise/sunset from the city's catalogue latitude — so
/// high-latitude cities like Reykjavík correctly stay "night"
/// through a winter noon-twilight, and "day" through a polar summer
/// night.
fn draw_hero_clock(
    frame: &mut Frame,
    app: &App,
    utc_now: &DateTime<Utc>,
    area: Rect,
    variant: HeroVariant,
    tc: &ThemeColors,
) {
    let now = utc_now.with_timezone(&app.selection.tz);
    let is_day = is_daytime_at(app.selection.tz, &now);
    let clock_color = if is_day {
        tc.accent
    } else {
        tc.accent_secondary
    };

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
        (area.height - grid_rows as u16 * panel_height) / 2
    } else {
        0
    };

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
    let is_day = entry
        .coordinates
        .map(|place| is_daytime_at_coordinates(place, &local));

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

    let time_style = match is_day {
        Some(true) => Style::default().fg(tc.good).add_modifier(Modifier::BOLD),
        Some(false) => Style::default().fg(tc.muted).add_modifier(Modifier::BOLD),
        // A fixed offset has no daylight, so neither the day nor the
        // night colour would be true of it.
        None => Style::default().fg(tc.fg).add_modifier(Modifier::BOLD),
    };
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
        InputMode::Normal => " q:quit  /:add  f:remove  J/K:reorder  t:theme  ?:help",
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

        assert!(rendered.contains(" Tokyo "), "got:\n{rendered}");
        assert!(rendered.contains(" London "), "got:\n{rendered}");
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
        assert!(rendered.contains("UTC"), "got:\n{rendered}");
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
}
