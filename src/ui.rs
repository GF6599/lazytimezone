//! # Terminal UI rendering.
//!
//! Reads [`App`] state and writes a ratatui [`Frame`]. It handles no
//! input. It writes two values back to `App`, because only the layout
//! knows them: the number of panel columns, which the arrow keys need,
//! and the help scroll limit.
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
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use unicode_width::UnicodeWidthStr;

use crate::app::{App, CopyStatus, InputMode, WallView};
use crate::digits;
use crate::land;
use crate::octant;
use crate::sky;
use crate::theme::ThemeColors;
use crate::timezone::{
    Coordinates, TimezoneEntry, daylight_margin, daylight_margins, format_utc_offset,
    is_daytime_at, is_daytime_at_coordinates, sun_times,
};

/// Top-level render entry point. Splits the frame into the title bar,
/// the hero clock, the wall and the status bar.
pub fn draw(frame: &mut Frame, app: &mut App) {
    draw_at(frame, app, Utc::now());
}

/// Draws the frame as it looks at `now`. Every child draw takes this one
/// instant, so every part of the UI agrees on "now", and a test can pick
/// the instant.
fn draw_at(frame: &mut Frame, app: &mut App, now: DateTime<Utc>) {
    let tc = app.theme.colors();

    let (hero, hero_height) = hero_variant(frame.area());
    app.set_clock_draws_art(matches!(hero, HeroVariant::Art { .. }));
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
    ("a", "Turn the clock animation off and on"),
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

/// Renders a centered help popup listing every keybinding, grouped by
/// mode. The arrows scroll it, and any other key closes it.
///
/// [`Clear`] wipes the cells underneath first, so the background does
/// not show through the popup's borders.
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
    // and padding, with no manual width arithmetic required.
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

/// How long a changed digit of the main clock takes to roll into place.
const ROLL: Duration = Duration::from_millis(250);
/// The frame interval while the main clock moves, 60 frames a second.
const MOTION_FRAME: Duration = Duration::from_micros(16_667);
/// How far into each second the colons start to fade to dim.
const COLON_DIM_AT: Duration = Duration::from_millis(500);
/// How long the colons take to fade between bright and dim. The fade
/// back to bright starts with the second, inside the roll.
const COLON_FADE: Duration = Duration::from_millis(150);
const SECOND: Duration = Duration::from_secs(1);

/// How far `now` is into its second. A leap second counts as the end of
/// the second before it.
fn into_second(now: &DateTime<Utc>) -> Duration {
    Duration::from_nanos(u64::from(now.timestamp_subsec_nanos()))
        .min(SECOND - Duration::from_nanos(1))
}

/// How long the loop can wait at `now` before the main clock looks
/// different. The frames land on wall-clock second boundaries, so the
/// clock changes when the second does.
pub fn until_next_frame(now: DateTime<Utc>, animated: bool) -> Duration {
    let into = into_second(&now);
    if animated {
        for moving in [
            Duration::ZERO..ROLL,
            COLON_DIM_AT..COLON_DIM_AT + COLON_FADE,
        ] {
            if moving.contains(&into) {
                return MOTION_FRAME.min(moving.end - into);
            }
            if into < moving.start {
                return moving.start - into;
            }
        }
    }
    SECOND - into
}

/// The colour of the colons `into` a second: a fade from `dim` to
/// `bright` as the second starts, and back to `dim` from
/// [`COLON_DIM_AT`].
fn colon_colour(into: Duration, bright: Color, dim: Color) -> Color {
    let fade = |from, to, since: Duration| {
        octant::mix(from, to, since.div_duration_f64(COLON_FADE) as f32)
    };
    if into < COLON_DIM_AT {
        fade(dim, bright, into)
    } else {
        fade(bright, dim, into - COLON_DIM_AT)
    }
}

/// How far the top of each digit shades toward the text colour, and the
/// bottom toward the border colour, both of the theme.
const DIGIT_SHADE: f32 = 0.35;

/// Fast at the start and slow into place, the way a wheel settles.
fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
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

    let into = into_second(utc_now);
    let (before, progress) = if app.clock_animation && into < ROLL {
        let before = (now - TimeDelta::seconds(1)).format("%H:%M:%S").to_string();
        (before, ease_out(into.div_duration_f64(ROLL)))
    } else {
        (time_str.clone(), 0.0)
    };
    let colon_color = if app.clock_animation {
        colon_colour(into, clock_color, tc.muted)
    } else {
        clock_color
    };
    let top = octant::mix(clock_color, tc.fg, DIGIT_SHADE);
    let bottom = octant::mix(clock_color, tc.border, DIGIT_SHADE);
    // One shade for each pixel row, mixed once a frame and not once a
    // pixel. A digit pixel `down` its glyph lies in row `down * tall`.
    let tall = usize::from(rows) * octant::CELL_HEIGHT;
    let shades: Vec<Color> = (0..tall)
        .map(|row| octant::mix(top, bottom, ((row as f64 + 0.5) / tall as f64) as f32))
        .collect();
    let mut art = octant::Canvas::new(clock_area.width, clock_area.height);
    digits::paint(
        &mut art,
        &before,
        &time_str,
        rows,
        progress,
        |part| match part {
            digits::Part::Colon => colon_color,
            digits::Part::Digit { down } => shades[((down * tall as f64) as usize).min(tall - 1)],
        },
    );
    art.render(clock_area, frame.buffer_mut());

    let meta_line = Paragraph::new(Line::from(Span::styled(
        meta,
        Style::default().fg(tc.muted),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(tc.bg));
    frame.render_widget(meta_line, chunks[3]);
}

/// How long the status bar shows the result of a copy.
const COPY_FLASH_FOR: std::time::Duration = std::time::Duration::from_secs(3);

/// Every theme shows a failure in this red, so it stands out against
/// each palette. `ThemeColors` has no slot for it.
const FAILURE_RED: Color = Color::Rgb(220, 80, 80);

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
    let colours = sky::SkyColours {
        dim: tc.subtle,
        middle: octant::mix(tc.subtle, tc.fg, 0.5),
        bright: tc.fg,
        lit: tc.subtle,
        shade: tc.muted,
    };
    let mut sky = octant::Canvas::new(band.width, band.height);
    sky::paint(&mut sky, utc_now.timestamp(), is_day, &colours);
    sky.render(band, frame.buffer_mut());
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
    let has_strip = inner.width >= STRIP_HOURS && inner.height >= 3;
    body.truncate(usize::from(inner.height) - usize::from(has_strip));
    frame.render_widget(
        Paragraph::new(body).style(Style::default().bg(tc.bg)),
        inner,
    );
    if has_strip {
        let mut strip = octant::Canvas::new(STRIP_HOURS, 1);
        paint_day_strip(
            &mut strip,
            entry.coordinates,
            entry.tz,
            app.selection.tz,
            utc_now,
            tc,
        );
        let row = Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        };
        strip.render(row, frame.buffer_mut());
    }

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

/// How long the twilight blend takes, centred on sunrise and on sunset.
const TWILIGHT_HOURS: f64 = 1.0;
/// The steps of light from night to day, as many as the canvas samples a
/// pixel's coverage in, so no step between two shows.
const LIGHT_STEPS: u16 = 16;

/// The light of a place `margin` hours inside the day, from 0 at night to
/// 1 by day, blended over [`TWILIGHT_HOURS`] round sunrise and sunset in
/// [`LIGHT_STEPS`] steps.
fn light_of(margin: f64) -> f32 {
    let share = (margin / TWILIGHT_HOURS + 0.5).clamp(0.0, 1.0);
    let steps = f64::from(LIGHT_STEPS);
    ((share * steps).round() / steps) as f32
}

/// One cell for each hour of the main clock's day, and one pixel column
/// for each half hour, coloured by the light at `place` in the middle of
/// that half hour: the day colour, the night colour, or their blend for
/// [`TWILIGHT_HOURS`] round sunrise and sunset. Every panel samples the
/// same instants, so one column is one moment on the whole wall. A place
/// with no daylight shows one small dot for each hour. The column of now
/// is in the marker colour.
fn paint_day_strip(
    canvas: &mut octant::Canvas,
    place: Option<Coordinates>,
    tz: Tz,
    hero_tz: Tz,
    utc_now: &DateTime<Utc>,
    tc: &ThemeColors,
) {
    let hero_now = utc_now.with_timezone(&hero_tz);
    let hero_midnight =
        *utc_now - TimeDelta::seconds(i64::from(hero_now.num_seconds_from_midnight()));
    let columns = usize::from(STRIP_HOURS) * octant::CELL_WIDTH;
    let minutes_per_column = 24 * 60 / columns as i64;
    for column in 0..columns {
        let middle = hero_midnight
            + TimeDelta::minutes(column as i64 * minutes_per_column + minutes_per_column / 2);
        let Some(place) = place else {
            if column % octant::CELL_WIDTH == 0 {
                canvas.set(column, octant::CELL_HEIGHT / 2, tc.fg);
                canvas.set(column + 1, octant::CELL_HEIGHT / 2, tc.fg);
            }
            continue;
        };
        let margin = daylight_margin(place, &middle.with_timezone(&tz));
        let colour = octant::mix(tc.muted, tc.good, light_of(margin));
        for y in 0..octant::CELL_HEIGHT {
            canvas.set(column, y, colour);
        }
    }
    let now = (i64::from(hero_now.num_seconds_from_midnight()) / 60 / minutes_per_column) as usize;
    for y in 0..octant::CELL_HEIGHT {
        canvas.set(now, y, tc.star);
    }
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
/// 2 by 4 pixels a cell, with a cell twice as tall as it is wide.
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
    let mut map = octant::Canvas::new(map_width, map_rows);
    paint_map(&mut map, utc_now, tc);
    map.render(map_area, frame.buffer_mut());
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

/// Paints the map over the whole of `canvas` as `utc_now` lights it:
/// land in the day colour, the night colour, or a blend of the two at the
/// terminator, and the sea on the night side as a stipple on every other
/// pixel across and down, so night reads over the terminal's own
/// background and day stays clear.
///
/// The light comes from [`daylight_margins`] on the UTC clock, the same
/// sun model the panels and day strips use, so a city's land and its
/// panel always agree on whether it is night.
fn paint_map(canvas: &mut octant::Canvas, utc_now: &DateTime<Utc>, tc: &ThemeColors) {
    let (width, height) = (canvas.width(), canvas.height());
    let m = map_colours(tc);
    let layers = map_layers(width, height);
    let latitudes: Vec<f64> = (0..height).map(|y| map_latitude(height, y)).collect();
    let longitudes: Vec<f64> = (0..width).map(|x| map_longitude(width, x)).collect();
    let margins = daylight_margins(&latitudes, &longitudes, &utc_now.with_timezone(&Tz::UTC));
    for y in 0..height {
        for x in 0..width {
            let light = light_of(margins[y * width + x]);
            let at = y * width + x;
            match layers.ground[at] {
                Ground::Land | Ground::Coast => {
                    let land = octant::mix(m.night, m.day, light);
                    let glow = layers.glow[at];
                    let colour = if light < 0.5 && glow > 0.0 {
                        city_light(land, glow, tc)
                    } else if layers.ground[at] == Ground::Coast {
                        coast_colour(land, tc)
                    } else {
                        land
                    };
                    canvas.set(x, y, colour);
                }
                Ground::Equator => canvas.set(x, y, m.equator),
                Ground::Graticule => canvas.set(x, y, m.grid),
                Ground::Sea if light < 0.5 && x % 2 == 0 && y % 2 == 0 => {
                    canvas.set(x, y, m.stipple);
                }
                Ground::Sea => {}
            }
        }
    }
}

/// The colours of the map, from the theme.
#[derive(Debug, Clone, Copy)]
struct MapColours {
    day: Color,
    night: Color,
    /// The sea on the night side, on every other pixel across and down.
    stipple: Color,
    /// A line of the graticule, every [`GRATICULE_DEGREES`].
    grid: Color,
    equator: Color,
}

fn map_colours(tc: &ThemeColors) -> MapColours {
    MapColours {
        day: tc.good,
        night: tc.accent_secondary,
        stipple: tc.border,
        grid: octant::mix(tc.border, tc.muted, 0.4),
        equator: tc.muted,
    }
}

/// The degrees between two lines of the map's graticule.
const GRATICULE_DEGREES: f64 = 30.0;
/// How far a coast pixel lightens toward the text colour.
const COAST_LIGHT: f32 = 0.3;
/// How far the smallest and the largest lit city lighten their pixel
/// toward the text colour. The smallest lies past [`COAST_LIGHT`], so a
/// light never reads as coast.
const CITY_LIGHT: std::ops::RangeInclusive<f32> = 0.45..=0.85;

/// The pixel of each graticule line from `low` to `high` degrees on an
/// axis of `count` pixels, where `pixel` places a degree on the axis.
fn graticule_pixels(count: usize, low: f64, high: f64, pixel: impl Fn(f64) -> f64) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut degrees = (low / GRATICULE_DEGREES).ceil() * GRATICULE_DEGREES;
    while degrees <= high {
        let at = pixel(degrees).round();
        if at >= 0.0 && (at as usize) < count {
            lines.push(at as usize);
        }
        degrees += GRATICULE_DEGREES;
    }
    lines
}

/// `land` on the coast, a step toward the text colour.
fn coast_colour(land: Color, tc: &ThemeColors) -> Color {
    octant::mix(land, tc.fg, COAST_LIGHT)
}

/// `land` at night under a city that glows `glow`, from just above 0 for
/// the smallest lit city to 1 for the largest.
fn city_light(land: Color, glow: f32, tc: &ThemeColors) -> Color {
    let (low, high) = (*CITY_LIGHT.start(), *CITY_LIGHT.end());
    octant::mix(land, tc.fg, low + (high - low) * glow)
}

/// What lies under a pixel of the map, before the time of day colours it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ground {
    Land,
    /// Land next to the sea or the map's edge.
    Coast,
    /// Sea on the equator.
    Equator,
    /// Sea on another line of the graticule.
    Graticule,
    Sea,
}

/// What the map holds at one size: the ground under each pixel, and how
/// brightly the cities in it glow at night, both row by row.
#[derive(Debug)]
struct MapLayers {
    ground: Vec<Ground>,
    glow: Vec<f32>,
}

/// The layers of the map, with the size in pixels they were made for.
type SizedLayers = ((usize, usize), Rc<MapLayers>);

thread_local! {
    /// The layers of the map at the last size it was drawn at. They
    /// depend only on the size, and making them costs more than the rest
    /// of a map frame.
    static MAP_LAYERS: RefCell<Option<SizedLayers>> = const { RefCell::new(None) };
}

/// The layers of a map `width` by `height` pixels, made once for each
/// size.
fn map_layers(width: usize, height: usize) -> Rc<MapLayers> {
    MAP_LAYERS.with_borrow_mut(|kept| match kept {
        Some((size, layers)) if *size == (width, height) => Rc::clone(layers),
        _ => {
            let land = land::mask(
                width,
                height,
                |x| map_longitude(width, x),
                |y| map_latitude(height, y),
            );
            let layers = Rc::new(MapLayers {
                glow: city_glow(width, height, &land),
                ground: ground(width, height, &land),
            });
            *kept = Some(((width, height), Rc::clone(&layers)));
            layers
        }
    })
}

/// The ground under each pixel of a map `width` by `height` pixels, from
/// whether each is `land`: land next to the sea or the edge is coast, and
/// sea on a line of the graticule is that line.
fn ground(width: usize, height: usize, land: &[bool]) -> Vec<Ground> {
    let meridians = graticule_pixels(width, -180.0, 180.0, |longitude| {
        (longitude + 180.0) / 360.0 * (width - 1) as f64
    });
    let parallels = graticule_pixels(height, MAP_SOUTH, MAP_NORTH, |latitude| {
        (MAP_NORTH - latitude) / (MAP_NORTH - MAP_SOUTH) * (height - 1) as f64
    });
    let equator = (MAP_NORTH / (MAP_NORTH - MAP_SOUTH) * (height - 1) as f64).round() as usize;
    let is_land = |x: usize, y: usize| land[y * width + x];
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            if is_land(x, y) {
                let shore = x == 0
                    || y == 0
                    || x + 1 == width
                    || y + 1 == height
                    || !is_land(x - 1, y)
                    || !is_land(x + 1, y)
                    || !is_land(x, y - 1)
                    || !is_land(x, y + 1);
                if shore { Ground::Coast } else { Ground::Land }
            } else if y == equator {
                Ground::Equator
            } else if parallels.contains(&y) || meridians.contains(&x) {
                Ground::Graticule
            } else {
                Ground::Sea
            }
        })
        .collect()
}

/// How brightly each land pixel of a map `width` by `height` pixels glows
/// at night: the largest catalogue city in it, from 0 below a hundred
/// thousand people to 1 at ten million, in [`LIGHT_STEPS`] steps. A city
/// at sea glows nowhere.
fn city_glow(width: usize, height: usize, land: &[bool]) -> Vec<f32> {
    let mut glow = vec![0.0_f32; width * height];
    let across = (width.max(2) - 1) as f64;
    let down = (height.max(2) - 1) as f64;
    for entry in crate::timezone::all_timezones() {
        let Some(place) = entry.coordinates else {
            continue;
        };
        let latitude = place.latitude.clamp(MAP_SOUTH, MAP_NORTH);
        let x = ((place.longitude + 180.0) / 360.0 * across).round() as usize;
        let y = ((MAP_NORTH - latitude) / (MAP_NORTH - MAP_SOUTH) * down).round() as usize;
        let index = y * width + x;
        if x >= width || y >= height || !land[index] {
            continue;
        }
        let size = ((entry.population.max(1) as f64).log10() - 5.0) / 2.0;
        let steps = f64::from(LIGHT_STEPS);
        let level = ((size.clamp(0.0, 1.0) * steps).round() / steps) as f32;
        glow[index] = glow[index].max(level);
    }
    glow
}

/// The longitude of pixel column `x` of a map `width` pixels wide, from
/// -180 at the left edge to 180 at the right. [`map_dot_cell`] places a
/// favourite on the same grid, so its dot sits on its own land.
fn map_longitude(width: usize, x: usize) -> f64 {
    -180.0 + x as f64 * 360.0 / (width.max(2) - 1) as f64
}

/// The latitude of pixel row `y` of a map `height` pixels tall, from
/// [`MAP_NORTH`] at the top edge to [`MAP_SOUTH`] at the bottom.
fn map_latitude(height: usize, y: usize) -> f64 {
    MAP_NORTH - y as f64 * (MAP_NORTH - MAP_SOUTH) / (height.max(2) - 1) as f64
}

/// The cell of `map_area` that holds the map pixel for (longitude,
/// latitude), on the grid of [`map_longitude`] and [`map_latitude`], so a
/// favourite's dot sits on its own land. Placing text by cells and
/// truncating drifts it up to a cell north-west.
fn map_dot_cell(map_area: Rect, longitude: f64, latitude: f64) -> (u16, u16) {
    let dots_across = f64::from(map_area.width) * 2.0;
    let dots_down = f64::from(map_area.height) * 4.0;
    let dot_x = ((longitude + 180.0) * (dots_across - 1.0) / 360.0).round();
    let dot_y = ((MAP_NORTH - latitude) * (dots_down - 1.0) / (MAP_NORTH - MAP_SOUTH)).round();
    (map_area.x + dot_x as u16 / 2, map_area.y + dot_y as u16 / 4)
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

    // The query scrolls so the cursor stays one column inside the right
    // edge.
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

    // The `hint:` suffix comes last, so a narrow terminal cuts it first
    // and the key bindings stay visible.
    //   * Stays under ~100 chars so it survives terminal widths down to ~80.
    let hints = match app.input_mode {
        InputMode::Normal => " q:quit  /:add  f:remove  J/K:reorder  t:theme  m:map  ?:help",
        InputMode::Search => {
            " Enter:add  Esc:close  Ctrl-u:clear  hint: city \u{00b7} +5:30 \u{00b7} asia"
        }
    };
    let hint_span = Span::styled(hints, Style::default().fg(tc.muted));

    let mut spans = vec![mode_span, hint_span];

    if let Some(flash) = &app.copy_flash
        && flash.started_at.elapsed() < COPY_FLASH_FOR
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
                        .fg(FAILURE_RED)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
    }

    // Surface startup load warnings / parse errors for the first 10s
    // after launch (the user can also dismiss them with any keypress,
    // see `App::dismiss_startup_messages`). Render them after the copy
    // flash so the most recent feedback wins the rightmost slot.
    if !app.startup_messages.is_empty()
        && app.started_at.elapsed() < std::time::Duration::from_secs(10)
    {
        let joined = app.startup_messages.join("; ");
        spans.push(Span::styled(
            format!(" \u{26A0} {joined}"),
            Style::default()
                .fg(FAILURE_RED)
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
/// (e.g. London/Lisbon both at UTC+0 in winter: distinct DST rules,
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
    // Tests panic on failure by design. See src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

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
    /// in cells), so a test can tell one art scale from another. Only the
    /// rows of the hero zone count, so the sky and a day strip are not art.
    fn hero_ink(rows: &[String]) -> (usize, usize) {
        let width = rows.first().map_or(0, |row| row.chars().count());
        let area = Rect::new(0, 0, width as u16, rows.len() as u16);
        let hero_rows = 1 + usize::from(hero_variant(area).1);
        let mut ink_rows = 0;
        let mut min_col = usize::MAX;
        let mut max_col = 0;
        for row in rows.iter().take(hero_rows) {
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
            favorites: vec![config::SavedCity::City {
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
        // One column and room for three panel rows force scrolling.
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
    fn a_tall_terminal_draws_the_hero_clock_as_art() {
        let mut app = wall_app(&["tokyo"]);

        let rows = render_app(&mut app, 100, 40);

        let (ink_rows, _) = hero_ink(&rows);
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

    /// 12:34:57 UTC, `millis` into the second.
    fn into_twelve_thirty_four_fifty_seven(millis: i64) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 6, 21, 12, 34, 57).unwrap() + TimeDelta::milliseconds(millis)
    }

    /// The symbols of the whole frame, one string per row.
    fn symbols(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// Each hero art cell with its place and foreground. At 80x24 the art
    /// is 7 rows tall and starts under the title bar and one gap row.
    fn hero_art_cells(buffer: &Buffer) -> Vec<((u16, u16), Color)> {
        let mut cells = Vec::new();
        for y in 2..9 {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol().chars().any(is_art_glyph) {
                    cells.push(((x, y), cell.fg));
                }
            }
        }
        cells
    }

    /// The foreground of each hero art cell.
    fn hero_art_colours(buffer: &Buffer) -> Vec<Color> {
        hero_art_cells(buffer)
            .into_iter()
            .map(|(_, colour)| colour)
            .collect()
    }

    /// How light `colour` looks, from its sRGB channels.
    fn luma(colour: Color) -> f32 {
        let Color::Rgb(r, g, b) = colour else {
            panic!("{colour:?} has no RGB value");
        };
        0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
    }

    /// The main clock rolls a changed digit into place.
    ///
    /// Given: the clock with its animation on, and the same clock with
    /// its animation off.
    /// When: both are drawn 100 ms after the seconds change, and again
    /// 400 ms after.
    /// Then: the two differ at 100 ms, and draw the same digits at
    /// 400 ms.
    ///
    /// Why it matters: the roll must finish well inside its second, so
    /// the clock is still for most of the time and easy to read.
    #[test]
    fn the_main_clock_rolls_a_changed_digit_into_place() {
        // Given:
        let mut animated = test_app();
        let mut still = test_app();
        still.clock_animation = false;

        // When:
        let draw = |app: &mut App, millis| {
            symbols(&render_buffer_at(
                app,
                80,
                24,
                into_twelve_thirty_four_fifty_seven(millis),
            ))
        };
        let (rolling, at_rest) = (draw(&mut animated, 100), draw(&mut still, 100));
        let (rolled, settled) = (draw(&mut animated, 400), draw(&mut still, 400));

        // Then:
        assert_ne!(rolling, at_rest, "no roll:\n{}", rolling.join("\n"));
        assert_eq!(rolled, settled, "the roll did not settle");
    }

    /// The colons dim in the second half of each second.
    ///
    /// Given: the clock with its animation on, and the same clock with
    /// its animation off.
    /// When: each is drawn 200 ms and 700 ms into a second.
    /// Then: only the animated clock at 700 ms draws art in the muted
    /// colour, and its digits keep their own colour.
    ///
    /// Why it matters: the pulse marks the seconds passing. A clock
    /// with its animation off must hold still.
    #[test]
    fn the_colons_dim_in_the_second_half_of_each_second() {
        // Given:
        let mut animated = test_app();
        let mut still = test_app();
        still.clock_animation = false;
        let muted = animated.theme.colors().muted;

        // When:
        let colours = |app: &mut App, millis| {
            hero_art_colours(&render_buffer_at(
                app,
                80,
                24,
                into_twelve_thirty_four_fifty_seven(millis),
            ))
        };
        let bright = colours(&mut animated, 200);
        let pulsed = colours(&mut animated, 700);
        let held = colours(&mut still, 700);

        // Then:
        assert!(!pulsed.is_empty(), "no hero art at 80x24");
        assert!(!bright.contains(&muted), "the colons dimmed early");
        assert!(pulsed.contains(&muted), "the colons did not dim");
        assert!(
            pulsed.iter().any(|&c| c != muted),
            "the digits dimmed with the colons"
        );
        assert!(!held.contains(&muted), "a still clock pulsed");
    }

    /// The colons fade between bright and dim.
    ///
    /// Given: the clock with its animation on, and its colon cells, the
    /// cells in the muted colour 700 ms into a second.
    /// When: it is drawn half way through the fade to dim, and half way
    /// through the fade back to bright.
    /// Then: each time every colon cell is in a colour between the clock
    /// colour and the muted colour.
    ///
    /// Why it matters: a colour that switches in one frame blinks. A fade
    /// reads as a pulse.
    #[test]
    fn the_colons_fade_between_bright_and_dim() {
        // Given:
        let mut app = test_app();
        let tc = app.theme.colors();
        let mut cells = |millis| {
            let at = into_twelve_thirty_four_fifty_seven(millis);
            (
                clock_colour(&app, &at, &tc),
                hero_art_cells(&render_buffer_at(&mut app, 80, 24, at)),
            )
        };
        let (_, dim) = cells(700);
        let colons: Vec<(u16, u16)> = dim
            .into_iter()
            .filter(|(_, c)| *c == tc.muted)
            .map(|(at, _)| at)
            .collect();
        assert!(!colons.is_empty(), "no colon dims at 700 ms");

        for millis in [575, 75] {
            // When:
            let (clock, drawn) = cells(millis);

            // Then:
            for (at, colour) in drawn.into_iter().filter(|(at, _)| colons.contains(at)) {
                assert!(
                    colour != clock && colour != tc.muted,
                    "{millis} ms: colon cell {at:?} is {colour:?}"
                );
            }
        }
    }

    /// The digits shade from a lighter top to a deeper bottom.
    ///
    /// Given: the clock at rest, with its animation on and off.
    /// When: each is drawn 400 ms into a second, after the roll.
    /// Then: every digit cell in the top row of the art is lighter than
    /// every digit cell in the bottom row.
    ///
    /// Why it matters: the shade gives the digits depth, and a still clock
    /// keeps it, because it is no motion.
    #[test]
    fn the_digits_shade_from_a_lighter_top_to_a_deeper_bottom() {
        for animated in [true, false] {
            // Given:
            let mut app = test_app();
            app.clock_animation = animated;

            // When:
            let at = into_twelve_thirty_four_fifty_seven(400);
            let cells = hero_art_cells(&render_buffer_at(&mut app, 80, 24, at));

            // Then:
            let row = |y| {
                cells
                    .iter()
                    .filter(move |((_, cy), _)| *cy == y)
                    .map(|(_, c)| luma(*c))
            };
            let top = cells.iter().map(|((_, y), _)| *y).min().unwrap();
            let bottom = cells.iter().map(|((_, y), _)| *y).max().unwrap();
            let darkest_top = row(top).fold(f32::INFINITY, f32::min);
            let lightest_bottom = row(bottom).fold(0.0, f32::max);
            assert!(
                darkest_top > lightest_bottom,
                "animated {animated}: top {darkest_top} is not lighter than bottom {lightest_bottom}"
            );
        }
    }

    /// A clock drawn as text waits a whole second between frames.
    ///
    /// Given: the clock with its animation on.
    /// When: it is drawn on a 30-column terminal, where it is a line of
    /// text, and on an 80-column terminal, where it is art.
    /// Then: at the start of a second the text clock waits for the next
    /// second, and the art clock draws its next frame after
    /// [`MOTION_FRAME`].
    ///
    /// Why it matters: a line of text neither rolls nor fades, so motion
    /// frames there cost battery and show nothing.
    #[test]
    fn a_clock_drawn_as_text_waits_a_whole_second_between_frames() {
        // Given:
        let mut app = test_app();
        let start = into_twelve_thirty_four_fifty_seven(0);

        // When:
        render_buffer_at(&mut app, 30, 24, start);
        let as_text = until_next_frame(start, app.clock_moves());
        render_buffer_at(&mut app, 80, 24, start);
        let as_art = until_next_frame(start, app.clock_moves());

        // Then:
        assert_eq!(as_text, SECOND);
        assert_eq!(as_art, MOTION_FRAME);
    }

    /// The next frame comes when the main clock next changes.
    ///
    /// Given: instants across one second.
    /// When: the delay to the next frame is asked for, with the
    /// animation on and off.
    /// Then: an animated clock draws a frame every [`MOTION_FRAME`] while
    /// the digits roll and while the colons fade, one when each ends, and
    /// one when the seconds change. A still clock draws only when the
    /// seconds change.
    ///
    /// Why it matters: an idle clock wakes the process once a second, to
    /// keep battery use low. The extra frames come only while something
    /// moves.
    #[test]
    fn the_next_frame_comes_when_the_main_clock_next_changes() {
        // Given:
        let at = into_twelve_thirty_four_fifty_seven;
        let ms = std::time::Duration::from_millis;

        // When:
        let animated = |millis| until_next_frame(at(millis), true);
        let still = |millis| until_next_frame(at(millis), false);

        // Then:
        assert_eq!(animated(0), MOTION_FRAME);
        assert_eq!(animated(240), ms(10), "the frame that ends the roll");
        assert_eq!(animated(300), ms(200), "the frame that starts the fade");
        assert_eq!(animated(500), MOTION_FRAME);
        assert_eq!(animated(640), ms(10), "the frame that ends the fade");
        assert_eq!(animated(700), ms(300));
        assert_eq!(still(100), ms(900));
        assert_eq!(still(700), ms(300));
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

    /// One cell of a day strip: its glyph, foreground and background.
    type StripCell = (char, Color, Color);

    /// Whether a strip cell draws in `colour`, in front or behind.
    fn shows(cell: &StripCell, colour: Color) -> bool {
        cell.1 == colour || cell.2 == colour
    }

    /// Finds each panel's day strip as (row, first column, cells), in
    /// reading order. A strip is 24 art cells right after a panel's left
    /// border.
    fn day_strips(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16, Vec<StripCell>)> {
        let mut strips = Vec::new();
        for y in 0..buffer.area.height {
            let row: Vec<StripCell> = (0..buffer.area.width)
                .map(|x| {
                    let cell = &buffer[(x, y)];
                    (
                        cell.symbol().chars().next().unwrap_or(' '),
                        cell.fg,
                        cell.bg,
                    )
                })
                .collect();
            for x in 0..row.len().saturating_sub(24) {
                if row[x].0 == '\u{2502}'
                    && row[x + 1..x + 25].iter().all(|cell| is_art_glyph(cell.0))
                {
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
        let tc = app.theme.colors();
        let strips = day_strips(&buffer);
        assert_eq!(strips.len(), 2, "strips: {strips:?}");
        let (_, _, london) = &strips[0];
        let (_, _, tokyo) = &strips[1];
        assert!(
            shows(&london[21], tc.good) && shows(&london[8], tc.muted),
            "London: {london:?}"
        );
        assert!(
            shows(&tokyo[21], tc.muted) && shows(&tokyo[8], tc.good),
            "Tokyo: {tokyo:?}"
        );
        for (y, _, cells) in &strips {
            assert!(
                shows(&cells[21], tc.star),
                "now cell at row {y}: {:?}",
                cells[21]
            );
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
        let tc = app.theme.colors();
        let strips = day_strips(&buffer);
        assert_eq!(strips.len(), 1, "strips: {strips:?}");
        assert!(
            !strips[0]
                .2
                .iter()
                .any(|cell| shows(cell, tc.good) || shows(cell, tc.muted)),
            "cells: {:?}",
            strips[0].2
        );
    }

    /// A day strip blends through twilight from night to day.
    ///
    /// Given: London on the wall at noon UTC on 21 June.
    /// When: the wall is drawn with room for the strip.
    /// Then: some cell of London's strip, away from the now column, is a
    /// colour between the night colour and the day colour in lightness.
    ///
    /// Why it matters: a strip that steps straight from night to day puts
    /// sunrise on a whole hour, up to half an hour from where it is.
    #[test]
    fn a_day_strip_blends_through_twilight() {
        // Given:
        let mut app = wall_app(&["london"]);
        let tc = app.theme.colors();

        // When:
        let buffer = render_buffer_at(&mut app, 80, 40, midsummer_noon_utc());

        // Then:
        let strips = day_strips(&buffer);
        let london = &strips.first().expect("no strip").2;
        let (night, day) = (luma(tc.muted), luma(tc.good));
        let twilight = london
            .iter()
            .filter(|cell| !shows(cell, tc.star))
            .any(|cell| {
                let light = luma(cell.1);
                cell.1 != tc.muted && cell.1 != tc.good && night < light && light < day
            });
        assert!(twilight, "no twilight cell: {london:?}");
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
                .map(|&(city, admin1, cc)| config::SavedCity::City {
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

    /// The places of the night stipple on a map of `columns` by `rows`
    /// cells painted at `utc_now`.
    fn stipple_places(utc_now: DateTime<Utc>, columns: u16, rows: u16) -> Vec<Coordinates> {
        let tc = test_app().theme.colors();
        let mut map = octant::Canvas::new(columns, rows);
        paint_map(&mut map, &utc_now, &tc);
        let (width, height) = (map.width(), map.height());
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|&(x, y)| map.get(x, y) == Some(tc.border))
            .map(|(x, y)| Coordinates {
                latitude: map_latitude(height, y),
                longitude: map_longitude(width, x),
            })
            .collect()
    }

    /// At an equinox noon UTC the night side is the half of the world
    /// around the date line.
    ///
    /// Given: 20 March 2026 at 12:00 UTC, when the sun is overhead near
    /// 0° longitude.
    /// When: the map is painted.
    /// Then: every night stipple pixel lies more than 80° from the prime
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
        let night = stipple_places(equinox_noon, 60, 12);

        // Then:
        assert!(
            night.iter().all(|place| place.longitude.abs() > 80.0),
            "night near the prime meridian: {night:?}"
        );
        assert!(
            night.iter().any(|place| place.longitude.abs() > 170.0),
            "no night at the date line: {night:?}"
        );
    }

    /// At midsummer midnight UTC the far north is still in daylight.
    ///
    /// Given: 21 June 2026 at 00:00 UTC.
    /// When: the map is painted.
    /// Then: no night stipple pixel lies north of 70°, and night does
    /// reach the far south.
    ///
    /// Why it matters: the midnight sun is the case a plain "dark side
    /// of the globe" drawing gets wrong.
    #[test]
    fn the_arctic_has_no_night_at_midsummer_midnight() {
        // Given:
        use chrono::TimeZone;
        let midsummer_midnight = Utc.with_ymd_and_hms(2026, 6, 21, 0, 0, 0).unwrap();

        // When:
        let night = stipple_places(midsummer_midnight, 60, 12);

        // Then:
        assert!(
            night.iter().all(|place| place.latitude < 70.0),
            "night in the Arctic: {night:?}"
        );
        assert!(
            night.iter().any(|place| place.latitude < -40.0),
            "no night in the far south: {night:?}"
        );
    }

    /// A favourite's dot sits on the grid the land is painted on.
    ///
    /// Given: cities spread over the map, north and south, east and west,
    /// at three map sizes.
    /// When: each city's dot cell is computed.
    /// Then: the cell holds a pixel whose place on the land grid lies
    /// within half a pixel of the city in each direction.
    ///
    /// Why it matters: a dot placed by another grid than the land drifts
    /// off its coast. A text label placed by cells drifted up to a cell
    /// north-west, which put Sydney off Australia.
    #[test]
    fn a_favourites_dot_sits_on_the_grid_of_the_land() {
        let places = [
            ("London", -0.13, 51.51),
            ("Sydney", 151.21, -33.87),
            ("Tokyo", 139.69, 35.69),
            ("Buenos Aires", -58.38, -34.60),
            ("Nairobi", 36.82, -1.29),
            ("Tabwakea Village", -157.49, 2.02),
        ];
        for (columns, rows) in [(95_u16, 19_u16), (60, 12), (40, 8)] {
            // Given:
            let area = Rect::new(0, 0, columns, rows);
            let (width, height) = (
                usize::from(columns) * octant::CELL_WIDTH,
                usize::from(rows) * octant::CELL_HEIGHT,
            );
            let across = 360.0 / (width - 1) as f64;
            let down = (MAP_NORTH - MAP_SOUTH) / (height - 1) as f64;

            for (name, longitude, latitude) in places {
                // When:
                let (column, row) = map_dot_cell(area, longitude, latitude);

                // Then:
                let left = usize::from(column) * octant::CELL_WIDTH;
                let top = usize::from(row) * octant::CELL_HEIGHT;
                let near = (top..top + octant::CELL_HEIGHT).any(|y| {
                    (left..left + octant::CELL_WIDTH).any(|x| {
                        (map_longitude(width, x) - longitude).abs() <= across / 2.0
                            && (map_latitude(height, y) - latitude).abs() <= down / 2.0
                    })
                });
                assert!(
                    near,
                    "{name} on a {columns}x{rows} map is off the land grid"
                );
            }
        }
    }

    /// The land is solid inside a continent, a sea stays open, and the
    /// terminator blends on land.
    ///
    /// Given: a map of 95 by 19 cells at noon UTC on 21 June.
    /// When: it is painted.
    /// Then: the pixel over Chad and the pixel over Brazil are land, the
    /// pixel over the Ionian Sea is not, and some land pixel is a blend
    /// strictly between the night and the day colour.
    ///
    /// Why it matters: an outline gives no area to colour by day and
    /// night, a fill that leaked through a strait paints a sea as land, and
    /// a hard terminator steps across a continent.
    #[test]
    fn the_land_is_solid_a_sea_stays_open_and_the_terminator_blends() {
        // Given:
        let tc = test_app().theme.colors();
        let mut map = octant::Canvas::new(95, 19);

        // When:
        paint_map(&mut map, &midsummer_noon_utc(), &tc);

        // Then:
        let (width, height) = (map.width(), map.height());
        let pixel = |longitude: f64, latitude: f64| {
            let x = ((longitude + 180.0) / 360.0 * (width - 1) as f64).round() as usize;
            let y = ((MAP_NORTH - latitude) / (MAP_NORTH - MAP_SOUTH) * (height - 1) as f64).round()
                as usize;
            map.get(x, y).unwrap()
        };
        let land = |colour: Color| colour != Color::Reset && colour != tc.border;
        assert!(land(pixel(19.0, 15.0)), "Chad is not land");
        assert!(land(pixel(-50.0, -10.0)), "Brazil is not land");
        assert!(!land(pixel(18.0, 35.0)), "the Ionian Sea is land");
        let twilight = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .any(|(x, y)| {
                let colour = map.get(x, y).unwrap();
                land(colour) && colour != tc.good && colour != tc.accent_secondary
            });
        assert!(twilight, "no land in twilight");
    }

    /// The map fills its land again when the terminal changes size.
    ///
    /// Given: a map painted at 95 by 19 cells.
    /// When: a map is painted at 60 by 12 cells.
    /// Then: its land pixels are the land filled fresh for that size.
    ///
    /// Why it matters: the map keeps its land for its size, and land kept
    /// past a resize would draw the continents of the old size.
    #[test]
    fn the_map_fills_its_land_again_when_the_size_changes() {
        // Given:
        let tc = test_app().theme.colors();
        let m = map_colours(&tc);
        paint_map(&mut octant::Canvas::new(95, 19), &midsummer_noon_utc(), &tc);

        // When:
        let mut map = octant::Canvas::new(60, 12);
        paint_map(&mut map, &midsummer_noon_utc(), &tc);

        // Then:
        let (width, height) = (map.width(), map.height());
        let fresh = land::mask(
            width,
            height,
            |x| map_longitude(width, x),
            |y| map_latitude(height, y),
        );
        for y in 0..height {
            for x in 0..width {
                let colour = map.get(x, y).unwrap();
                let sea = [Color::Reset, m.stipple, m.grid, m.equator];
                assert_eq!(!sea.contains(&colour), fresh[y * width + x], "({x}, {y})");
            }
        }
    }

    /// A painted map of 95 by 19 cells at noon UTC on 21 June, with the
    /// pixel nearest each place.
    fn midsummer_map() -> (octant::Canvas, impl Fn(&octant::Canvas, f64, f64) -> Color) {
        let tc = test_app().theme.colors();
        let mut map = octant::Canvas::new(95, 19);
        paint_map(&mut map, &midsummer_noon_utc(), &tc);
        let pixel = |map: &octant::Canvas, longitude: f64, latitude: f64| {
            let (width, height) = (map.width(), map.height());
            let x = ((longitude + 180.0) / 360.0 * (width - 1) as f64).round() as usize;
            let y = ((MAP_NORTH - latitude) / (MAP_NORTH - MAP_SOUTH) * (height - 1) as f64).round()
                as usize;
            map.get(x, y).unwrap()
        };
        (map, pixel)
    }

    /// The coast is lit, and the land inside it is not.
    ///
    /// Given: the map at noon UTC on 21 June.
    /// When: it is painted.
    /// Then: every day land pixel next to the sea is the coast tint of the
    /// day colour, every other day land pixel is the day colour, and both
    /// show.
    ///
    /// Why it matters: the coast edge is hard against the terminal
    /// background, so a lighter rim is what makes a shape read sharp. A
    /// rim inland would draw false coasts.
    #[test]
    fn the_coast_is_lit_and_the_land_inside_it_is_not() {
        // Given:
        let tc = test_app().theme.colors();
        let m = map_colours(&tc);
        let (map, _) = midsummer_map();
        let (width, height) = (map.width(), map.height());
        let sea = |x: usize, y: usize| {
            let colour = map.get(x, y).unwrap();
            colour == Color::Reset || colour == m.stipple || colour == m.grid || colour == m.equator
        };

        // When, then:
        let (mut coast, mut inland) = (0, 0);
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let colour = map.get(x, y).unwrap();
                if colour != m.day && colour != coast_colour(m.day, &tc) {
                    continue;
                }
                let shore = sea(x - 1, y) || sea(x + 1, y) || sea(x, y - 1) || sea(x, y + 1);
                if shore {
                    assert_eq!(
                        colour,
                        coast_colour(m.day, &tc),
                        "({x}, {y}) is on the coast"
                    );
                    coast += 1;
                } else {
                    assert_eq!(colour, m.day, "({x}, {y}) is inland");
                    inland += 1;
                }
            }
        }
        assert!(coast > 0 && inland > 0, "coast {coast}, inland {inland}");
    }

    /// A faint graticule crosses the sea, with the equator a step brighter.
    ///
    /// Given: the map at noon UTC on 21 June, when the Atlantic is in day.
    /// When: it is painted.
    /// Then: the Atlantic pixel on 30°W at 10°N is the graticule colour,
    /// the pixel on the equator at 20°W is the equator colour, and the
    /// pixel at 35°W and 15°N, on no line, is clear.
    ///
    /// Why it matters: the lines give the eye a measure of where a city
    /// lies. A line off its degree would mislead that measure.
    #[test]
    fn a_faint_graticule_crosses_the_sea() {
        // Given:
        let m = map_colours(&test_app().theme.colors());

        // When:
        let (map, pixel) = midsummer_map();

        // Then:
        assert_eq!(pixel(&map, -30.0, 10.0), m.grid, "30°W");
        assert_eq!(pixel(&map, -20.0, 0.0), m.equator, "the equator");
        assert_eq!(pixel(&map, -35.0, 15.0), Color::Reset, "off the lines");
    }

    /// The cities light the night side, and the day side is not lit.
    ///
    /// Given: the map at noon UTC on 21 June, 21:00 in Seoul and 14:00 in
    /// Paris.
    /// When: it is painted.
    /// Then: Seoul's pixel is a city light, lighter than the night land,
    /// and Paris's pixel is plain day land.
    ///
    /// Why it matters: the lights show where people live at night. A light
    /// by day would hide which side of the terminator a city is on.
    #[test]
    fn the_cities_light_the_night_side() {
        // Given:
        let tc = test_app().theme.colors();
        let m = map_colours(&tc);

        // When:
        let (map, pixel) = midsummer_map();

        // Then:
        let seoul = pixel(&map, 126.98, 37.57);
        let lit = (1..=LIGHT_STEPS)
            .any(|step| seoul == city_light(m.night, step as f32 / LIGHT_STEPS as f32, &tc));
        assert!(lit, "Seoul is {seoul:?}");
        assert_ne!(seoul, coast_colour(m.night, &tc), "Seoul reads as coast");
        let paris = pixel(&map, 2.35, 48.86);
        assert!(
            paris == m.day || paris == coast_colour(m.day, &tc),
            "Paris is {paris:?}"
        );
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
        let below_hero = 1 + hero_variant(buffer.area).1;
        let map_bottom = (below_hero..buffer.area.height)
            .filter(|&y| {
                (0..buffer.area.width).any(|x| buffer[(x, y)].symbol().chars().any(is_art_glyph))
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

    /// The rows of the sky band: rows under the hero with art and no
    /// panel border, as (row, glyphs).
    fn sky_band(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, String)> {
        let below_hero = 1 + hero_variant(buffer.area).1;
        (below_hero..buffer.area.height)
            .map(|y| {
                let glyphs: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect();
                (y, glyphs)
            })
            .take_while(|(_, glyphs)| {
                !glyphs.contains('\u{2502}')
                    && !glyphs.contains('\u{256d}')
                    && !glyphs.contains('\u{250c}')
            })
            .filter(|(_, glyphs)| glyphs.chars().any(is_art_glyph))
            .collect()
    }

    /// The glyphs of each sky row.
    fn sky_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
        sky_band(buffer)
            .into_iter()
            .map(|(_, glyphs)| glyphs)
            .collect()
    }

    /// Each inked sky cell as (row, glyph, foreground, background).
    fn sky_cells(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, char, Color, Color)> {
        sky_band(buffer)
            .into_iter()
            .flat_map(|(y, _)| {
                (0..buffer.area.width).filter_map(move |x| {
                    let cell = &buffer[(x, y)];
                    let glyph = cell.symbol().chars().next()?;
                    is_art_glyph(glyph).then_some((y, glyph, cell.fg, cell.bg))
                })
            })
            .collect()
    }

    /// Whether a sky cell draws in `colour`, in front or behind.
    fn draws_in(cell: &(u16, char, Color, Color), colour: Color) -> bool {
        cell.2 == colour || cell.3 == colour
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

        let tc = app.theme.colors();

        // When:
        let cells = sky_cells(&render_buffer_at(&mut app, 100, 48, midsummer_noon_utc()));

        // Then:
        assert!(!cells.is_empty(), "no sky");
        assert!(
            !cells.iter().any(|cell| draws_in(cell, tc.muted)),
            "a cloud underside at night: {cells:?}"
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
        let sky = sky_cells(&render_buffer_at(&mut app, 100, 48, midsummer_noon_utc()));
        app.toggle_map();
        let map = render_buffer_at(&mut app, 100, 40, midsummer_noon_utc());

        // Then:
        assert!(
            sky.iter().any(|cell| draws_in(cell, tc.muted)),
            "no cloud by day: {sky:?}"
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

        let tc = app.theme.colors();

        // When:
        let before_buffer = render_buffer_at(&mut app, 100, 48, noon);
        let before = sky_rows(&before_buffer);
        let after = sky_rows(&render_buffer_at(
            &mut app,
            100,
            48,
            noon + TimeDelta::seconds(4),
        ));

        // Then:
        assert!(
            sky_cells(&before_buffer)
                .iter()
                .any(|cell| draws_in(cell, tc.muted)),
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
    /// Why it matters: the sky moves only when the second changes, which
    /// already draws a frame, so it costs no extra frames, and a star that
    /// returned at random would read as noise.
    #[test]
    fn stars_twinkle_on_a_two_second_beat() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let night = midsummer_noon_utc();

        // When:
        let mut at = |seconds| {
            sky_cells(&render_buffer_at(
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

    /// The map draws land on the day side in the day colour, land on the
    /// night side in the night colour, and a plain favourite's dot in
    /// the text colour.
    ///
    /// Given: London (selected) and Tokyo at noon UTC on 21 June, which
    /// is 13:00 in London and 21:00 in Tokyo, in the map view.
    /// When: the frame is drawn.
    /// Then: the land around London's dot is in the day colour, the land
    /// around Tokyo's dot is in the night colour, Tokyo's dot is in the
    /// text colour, and every other map cell is the dim night stipple or a
    /// land colour on the way from night to day.
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
        let m = map_colours(&tc);
        let sea = [m.stipple, m.grid, m.equator];
        let land_around = |(x, y, _): (u16, u16, Color)| -> Vec<Color> {
            (y.saturating_sub(1)..=y + 1)
                .flat_map(|row| (x.saturating_sub(2)..=x + 2).map(move |column| (column, row)))
                .filter_map(|pos| {
                    let glyph = buffer[pos].symbol().chars().next()?;
                    let colour = buffer[pos].fg;
                    (is_art_glyph(glyph) && !sea.contains(&colour)).then_some(colour)
                })
                .collect()
        };
        let day_land = land_around(london);
        assert!(!day_land.is_empty(), "no land around London");
        assert!(
            day_land
                .iter()
                .all(|&c| c == m.day || c == coast_colour(m.day, &tc)),
            "London: {day_land:?}"
        );
        let shares = || (0..=LIGHT_STEPS).map(|step| step as f32 / LIGHT_STEPS as f32);
        let lit = |land: Color| -> Vec<Color> {
            shares().map(|glow| city_light(land, glow, &tc)).collect()
        };
        let night_land = land_around(tokyo);
        assert!(!night_land.is_empty(), "no land around Tokyo");
        assert!(
            night_land
                .iter()
                .all(|&c| c == coast_colour(m.night, &tc) || lit(m.night).contains(&c)),
            "Tokyo: {night_land:?}"
        );
        let terminator: Vec<Color> = shares()
            .map(|light| octant::mix(m.night, m.day, light))
            .collect();
        let mut allowed = vec![Color::Reset, m.stipple, m.grid, m.equator];
        for &land in &terminator {
            allowed.push(land);
            allowed.push(coast_colour(land, &tc));
            allowed.extend(lit(land));
        }
        let on_the_terminator = |colour: Color| allowed.contains(&colour);
        let below_hero = 1 + hero_variant(buffer.area).1;
        for pos in buffer.area.positions() {
            let cell = &buffer[pos];
            if !cell.symbol().chars().any(is_art_glyph) || pos.y < below_hero {
                continue;
            }
            for colour in [cell.fg, cell.bg] {
                assert!(
                    on_the_terminator(colour),
                    "a map cell at {pos:?} in another colour: {cell:?}"
                );
            }
        }
    }

    /// The stars shine at three brightnesses, and a twinkle peaks in the
    /// text colour.
    ///
    /// Given: Tokyo on the main clock at night.
    /// When: the panel view is drawn at one second and the next.
    /// Then: every star is in the subtle colour, half way from subtle to
    /// the text colour, or the text colour, and each of the three shows.
    ///
    /// Why it matters: stars of one brightness read as a flat pattern, and
    /// a sky in a dim colour is close to invisible.
    #[test]
    fn the_stars_shine_at_three_brightnesses() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let tc = app.theme.colors();
        let night = midsummer_noon_utc();
        let middle = octant::mix(tc.subtle, tc.fg, 0.5);

        // When:
        let cells: Vec<_> = [night, night + TimeDelta::seconds(1)]
            .into_iter()
            .flat_map(|now| sky_cells(&render_buffer_at(&mut app, 100, 48, now)))
            .collect();

        // Then:
        for brightness in [tc.subtle, middle, tc.fg] {
            assert!(
                cells.iter().any(|cell| cell.2 == brightness),
                "no star in {brightness:?}"
            );
        }
        for cell in &cells {
            assert!(
                [tc.subtle, middle, tc.fg].contains(&cell.2),
                "star {cell:?}"
            );
        }
    }

    /// Every star is one of three sizes, and none is smaller than two
    /// pixels.
    ///
    /// Given: Tokyo on the main clock at night.
    /// When: the panel view is drawn at one second and the next.
    /// Then: every star cell lights two, four or six of its eight pixels,
    /// and each of the three sizes shows.
    ///
    /// Why it matters: a one-pixel star is too small to see at a normal
    /// font size, and stars of one size read as a flat pattern.
    #[test]
    fn every_star_is_one_of_three_sizes() {
        // Given:
        let mut app = wall_app(&["tokyo"]);
        let night = midsummer_noon_utc();

        // When:
        let cells: Vec<_> = [night, night + TimeDelta::seconds(1)]
            .into_iter()
            .flat_map(|now| sky_cells(&render_buffer_at(&mut app, 100, 48, now)))
            .collect();

        // Then:
        let lit = |glyph: char| {
            ratatui::symbols::pixel::OCTANTS
                .iter()
                .position(|&each| each == glyph)
                .map_or(0, |bits| bits.count_ones())
        };
        let sizes: Vec<u32> = cells.iter().map(|cell| lit(cell.1)).collect();
        for size in [2, 4, 6] {
            assert!(sizes.contains(&size), "no star of {size} pixels: {sizes:?}");
        }
        assert!(
            sizes.iter().all(|size| [2, 4, 6].contains(size)),
            "{sizes:?}"
        );
    }

    /// A cloud is lit on top and shaded underneath.
    ///
    /// Given: London on the main clock at noon UTC on 21 June.
    /// When: the panel view is drawn.
    /// Then: the highest cloud row draws in the subtle colour, and the
    /// lowest cloud row draws in the muted colour.
    ///
    /// Why it matters: the light on the top and the shade underneath give
    /// a cloud its volume. A flat cloud reads as a stripe.
    #[test]
    fn a_cloud_is_lit_on_top_and_shaded_underneath() {
        // Given:
        let mut app = wall_app(&["london"]);
        let tc = app.theme.colors();

        // When:
        let cells = sky_cells(&render_buffer_at(&mut app, 100, 48, midsummer_noon_utc()));

        // Then:
        let top = cells.iter().map(|cell| cell.0).min().expect("no cloud");
        let bottom = cells.iter().map(|cell| cell.0).max().expect("no cloud");
        assert!(
            cells
                .iter()
                .any(|cell| cell.0 == top && draws_in(cell, tc.subtle)),
            "the top row is not lit: {cells:?}"
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.0 == bottom && draws_in(cell, tc.muted)),
            "the bottom row is not shaded: {cells:?}"
        );
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
