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
//! The hero clock adapts to terminal width: narrow terminals
//! (<60 cols) show a plain-text fallback, wider ones use
//! [`BigText`](tui_big_text::BigText). The add-city picker renders
//! as a centered modal over the wall while search mode is active.

use chrono::offset::Offset;
use chrono::{DateTime, Utc};

use ratatui::{
    Frame,
    layout::Alignment,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::borrow::Cow;

use tui_big_text::{BigText, PixelSize};

use unicode_width::UnicodeWidthStr;

use crate::app::{App, CopyStatus, InputMode};
use crate::theme::ThemeColors;
use crate::timezone::{format_utc_offset, is_daytime_at, is_daytime_at_latitude};

/// Top-level render entry point — splits the frame into five vertical
/// zones and delegates to specialised draw functions.
///
/// `Utc::now()` is sampled once at the start of the frame and threaded
/// through to all child draws so every part of the UI agrees on "now".
pub fn draw(frame: &mut Frame, app: &mut App) {
    let tc = app.theme.colors();
    let now = Utc::now();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Length(8), // hero clock
            Constraint::Min(4),    // favorite wall
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    draw_title_bar(frame, app, outer[0], &tc);
    draw_hero_clock(frame, app, &now, outer[1], &tc);
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

/// Renders the large ASCII-art clock with optional side clocks.
///
/// ## Day/night colouring
///
/// Daytime uses `accent` (bright), nighttime uses `accent_secondary`
/// (dim). The day/night decision delegates to
/// [`is_daytime_at`](crate::timezone::is_daytime_at), which computes
/// sunrise/sunset from the city's curated latitude — so high-latitude
/// cities like Reykjavík correctly stay "night" through a winter
/// noon-twilight, and "day" through a polar summer night.
///
/// ## Side clocks
///
/// When the terminal is wide enough (>60 + 24n cols), up to 2
/// favourite timezones are shown as smaller Quadrant-pixel clocks
/// beside the main HalfHeight clock.
fn draw_hero_clock(
    frame: &mut Frame,
    app: &App,
    utc_now: &DateTime<Utc>,
    area: Rect,
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
        "{} \u{00b7} {} \u{00b7} {}",
        app.selection.city_name,
        date_str,
        format_utc_offset(offset_secs)
    );

    if area.width < 60 {
        let lines = vec![
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
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(2)])
        .split(area);

    // tui-big-text renders every glyph 8 cells wide, so the art can
    // be centered arithmetically: the widget itself is left-aligned.
    let art_width = (time_str.chars().count() as u16) * 8;
    let clock_area = Rect {
        x: chunks[0].x + chunks[0].width.saturating_sub(art_width) / 2,
        width: art_width.min(chunks[0].width),
        ..chunks[0]
    };

    let big_text = BigText::builder()
        .pixel_size(PixelSize::HalfHeight)
        .style(Style::default().fg(clock_color).bg(tc.bg))
        .lines(vec![time_str.into()])
        .build();
    frame.render_widget(big_text, clock_area);

    let meta_line = Paragraph::new(Line::from(Span::styled(
        meta,
        Style::default().fg(tc.muted),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(tc.bg));
    frame.render_widget(meta_line, chunks[1]);
}

const PANEL_WIDTH: u16 = 26;
const PANEL_HEIGHT: u16 = 4;

/// Renders the favorite wall: one framed panel per favorite, in the
/// user's order, wrapped into as many columns as the width allows.
/// The grid geometry is reported back to the app so `j`/`k` know how
/// far one vertical step moves.
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
    let columns = (area.width.saturating_sub(1) / PANEL_WIDTH).max(1) as usize;
    app.set_wall_columns(columns);

    let count = app.favorites.len();
    if count == 0 {
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "press / to add a city",
                Style::default().fg(tc.muted),
            )),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().bg(tc.bg));
        frame.render_widget(hint, area);
        return;
    }

    let visible_rows = (area.height / PANEL_HEIGHT).max(1) as usize;
    let selected_grid_row = app.selected_panel / columns;
    // Scroll only far enough to reveal the selection.
    let top_row = selected_grid_row.saturating_sub(visible_rows.saturating_sub(1));

    let first = top_row * columns;
    for (slot, panel_pos) in (first..count).take(visible_rows * columns).enumerate() {
        let grid_row = (slot / columns) as u16;
        let grid_col = (slot % columns) as u16;
        let rect = Rect {
            x: area.x + 1 + grid_col * PANEL_WIDTH,
            y: area.y + grid_row * PANEL_HEIGHT,
            width: PANEL_WIDTH - 1,
            height: PANEL_HEIGHT,
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
    let is_day = is_daytime_at_latitude(entry.latitude, &local);

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

    let time_style = if is_day {
        Style::default().fg(tc.good).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tc.muted).add_modifier(Modifier::BOLD)
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
    let body = vec![
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
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
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

        // The wall starts under the title bar (1) and hero clock (8).
        let panel_top_left = buffer[(1, 9)].clone();
        assert_eq!(panel_top_left.symbol(), "\u{250c}");
        assert_eq!(panel_top_left.fg, star);
        // The unselected panel keeps the plain border.
        let neighbour_top_left = buffer[(1 + PANEL_WIDTH, 9)].clone();
        assert_eq!(neighbour_top_left.symbol(), "\u{250c}");
        assert_eq!(neighbour_top_left.fg, border);
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

        let rendered = render_app(&mut app, 80, 12).join("\n");

        for glyph in ['\u{2588}', '\u{2580}', '\u{2584}'] {
            assert!(
                !rendered.contains(glyph),
                "block art on a 12-row terminal:\n{rendered}"
            );
        }
        assert!(rendered.contains("UTC"), "got:\n{rendered}");
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
