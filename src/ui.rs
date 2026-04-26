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
//! │ Title bar (1 row)        theme label (right) │
//! ├──────────────────────┬───────────────────────┤
//! │ Big clock (5 rows)   │ Side clocks (favs)    │
//! │ City + date (2 rows) │                       │
//! ├──────────────────────┴───────────────────────┤
//! │ Search bar (3 rows, bordered)                │
//! ├──────────────────────────────────────────────┤
//! │ Timezone table (fills remaining space)       │
//! │  City │ Country │ Region │ Time │ UTC │ Diff │
//! ├──────────────────────────────────────────────┤
//! │ Status bar (1 row)                           │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! The big clock area adapts to terminal width: narrow terminals
//! (<60 cols) show a plain-text fallback, wider ones use
//! [`BigText`](tui_big_text::BigText) with up to 2 side clocks for
//! favourite timezones.

use chrono::offset::Offset;
use chrono::{Timelike, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table, TableState,
    },
};

use tui_big_text::{BigText, PixelSize};

use unicode_width::UnicodeWidthStr;

use crate::app::{App, InputMode, format_utc_offset};
use crate::theme::ThemeColors;

/// Top-level render entry point — splits the frame into five vertical
/// zones and delegates to specialised draw functions.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let tc = app.theme.colors();

    if tc.bg != ratatui::style::Color::Reset {
        let bg_block = Block::default().style(Style::default().bg(tc.bg));
        frame.render_widget(bg_block, frame.area());
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Length(7), // big clock
            Constraint::Length(3), // search bar
            Constraint::Min(8),    // timezone table
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    draw_title_bar(frame, app, outer[0], &tc);
    draw_big_clock(frame, app, outer[1], &tc);
    draw_search_bar(frame, app, outer[2], &tc);
    draw_table(frame, app, outer[3], &tc);
    draw_status_bar(frame, app, outer[4], &tc);
}

fn draw_title_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect, tc: &ThemeColors) {
    let title = Span::styled(
        " lazytimezone",
        Style::default().fg(tc.title).add_modifier(Modifier::BOLD),
    );
    let theme_label = Span::styled(
        format!("{} ", app.theme.label()),
        Style::default().fg(tc.muted),
    );
    let line = Line::from(vec![
        title,
        Span::raw(
            " ".repeat(
                area.width
                    .saturating_sub(14 + app.theme.label().len() as u16 + 1)
                    as usize,
            ),
        ),
        theme_label,
    ]);
    let bar = Paragraph::new(line).style(Style::default().bg(tc.bg).fg(tc.fg));
    frame.render_widget(bar, area);
}

/// Renders the large ASCII-art clock with optional side clocks.
///
/// ## Day/night colouring
///
/// Hours 06:00–17:59 use `accent` (bright), all others use
/// `accent_secondary` (dim). This gives an at-a-glance visual cue
/// for whether it's daytime in the selected city.
///
/// ## Side clocks
///
/// When the terminal is wide enough (>60 + 24n cols), up to 2
/// favourite timezones are shown as smaller Quadrant-pixel clocks
/// beside the main HalfHeight clock.
fn draw_big_clock(frame: &mut Frame, app: &App, area: ratatui::layout::Rect, tc: &ThemeColors) {
    let utc_now = Utc::now();
    let now = utc_now.with_timezone(&app.selected_timezone);
    let hour = now.hour();
    let is_day = (6..18).contains(&hour);
    let clock_color = if is_day {
        tc.accent
    } else {
        tc.accent_secondary
    };

    let time_str = now.format("%H:%M:%S").to_string();
    let date_str = now.format("%A, %B %d, %Y").to_string();

    if area.width < 60 {
        let lines = vec![
            Line::from(Span::styled(
                format!("  {}", time_str),
                Style::default()
                    .fg(clock_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  {} - {}", app.selected_city_name, date_str),
                Style::default().fg(tc.muted),
            )),
        ];
        let p = Paragraph::new(lines).style(Style::default().bg(tc.bg));
        frame.render_widget(p, area);
        return;
    }

    // Determine how many side clocks fit: each needs ~24 cols
    let side_clock_width: u16 = 24;
    let main_min_width: u16 = 60;
    // Fetch extra candidates so filtering out the selected tz still leaves up to 2
    let fav_tzs = app.top_favorite_timezones(4);
    let side_tzs: Vec<_> = fav_tzs
        .iter()
        .filter(|(tz, _)| *tz != app.selected_timezone)
        .take(2)
        .collect();
    let available_for_sides = area.width.saturating_sub(main_min_width);
    let max_sides = (available_for_sides / side_clock_width) as usize;
    let num_sides = max_sides.min(side_tzs.len());

    // Split horizontally: main clock | side clocks
    let h_constraints = if num_sides > 0 {
        vec![
            Constraint::Min(main_min_width),
            Constraint::Length(side_clock_width * num_sides as u16),
        ]
    } else {
        vec![Constraint::Min(main_min_width)]
    };
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(h_constraints)
        .split(area);

    // Draw main clock
    let main_area = h_chunks[0];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(2)])
        .split(main_area);

    let clock_area = ratatui::layout::Rect {
        x: chunks[0].x + 2,
        width: chunks[0].width.saturating_sub(2),
        ..chunks[0]
    };

    let big_text = BigText::builder()
        .pixel_size(PixelSize::HalfHeight)
        .style(Style::default().fg(clock_color).bg(tc.bg))
        .lines(vec![time_str.into()])
        .build();
    frame.render_widget(big_text, clock_area);

    let date_line = Paragraph::new(Line::from(Span::styled(
        format!("  {} - {}", app.selected_city_name, date_str),
        Style::default().fg(tc.muted),
    )))
    .style(Style::default().bg(tc.bg));
    frame.render_widget(date_line, chunks[1]);

    // Draw side clocks
    if num_sides > 0 {
        let side_area = h_chunks[1];
        let side_constraints: Vec<Constraint> = (0..num_sides)
            .map(|_| Constraint::Length(side_clock_width))
            .collect();
        let side_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(side_constraints)
            .split(side_area);

        for (i, &&(tz, city)) in side_tzs.iter().take(num_sides).enumerate() {
            draw_side_clock(frame, &utc_now, tz, city, side_chunks[i], tc);
        }
    }
}

/// Renders a single favourite side clock: city label, Quadrant-pixel
/// time, and abbreviated date, separated by a vertical border line.
fn draw_side_clock(
    frame: &mut Frame,
    utc_now: &chrono::DateTime<Utc>,
    tz: chrono_tz::Tz,
    city: &str,
    area: ratatui::layout::Rect,
    tc: &ThemeColors,
) {
    let local = utc_now.with_timezone(&tz);
    let hour = local.hour();
    let is_day = (6..18).contains(&hour);
    let time_color = if is_day {
        tc.accent
    } else {
        tc.accent_secondary
    };

    let time_str = local.format("%H:%M").to_string();
    let date_str = local.format("%a, %b %d").to_string();

    // Layout: city label (1 row) | big text (4 rows) | date (1 row)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // city label
            Constraint::Length(4), // quadrant big text
            Constraint::Length(1), // date
            Constraint::Min(0),    // absorb remainder
        ])
        .split(area);

    // City label with left border indicator
    let city_line = Paragraph::new(Line::from(vec![
        Span::styled(" \u{2502} ", Style::default().fg(tc.border)),
        Span::styled(
            city,
            Style::default().fg(tc.star).add_modifier(Modifier::BOLD),
        ),
    ]))
    .style(Style::default().bg(tc.bg));
    frame.render_widget(city_line, chunks[0]);

    // BigText clock in Quadrant size
    let clock_inner = ratatui::layout::Rect {
        x: chunks[1].x + 3,
        width: chunks[1].width.saturating_sub(3),
        ..chunks[1]
    };
    let big_text = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(time_color).bg(tc.bg))
        .lines(vec![time_str.into()])
        .build();
    frame.render_widget(big_text, clock_inner);

    // Date line
    let date_line = Paragraph::new(Line::from(vec![
        Span::styled(" \u{2502} ", Style::default().fg(tc.border)),
        Span::styled(date_str, Style::default().fg(tc.muted)),
    ]))
    .style(Style::default().bg(tc.bg));
    frame.render_widget(date_line, chunks[2]);
}

fn draw_search_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect, tc: &ThemeColors) {
    let border_color = match app.input_mode {
        InputMode::Search => tc.accent,
        InputMode::Normal => tc.border,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Search ")
        .title_style(Style::default().fg(tc.title));

    let display = if app.search_query.is_empty() && app.input_mode != InputMode::Search {
        Span::styled("type / to search...", Style::default().fg(tc.muted))
    } else {
        Span::styled(&app.search_query, Style::default().fg(tc.fg))
    };

    let p = Paragraph::new(Line::from(display))
        .block(block)
        .style(Style::default().bg(tc.bg));
    frame.render_widget(p, area);

    if app.input_mode == InputMode::Search {
        let display_width = app.search_query[..app.cursor_position].width() as u16;
        let x = area.x.saturating_add(display_width).saturating_add(1);
        let y = area.y + 1;
        frame.set_cursor_position((x, y));
    }
}

/// Renders the scrollable timezone table with manual viewport
/// management.
///
/// ## Why manual viewport instead of ratatui's built-in scroll?
///
/// We only build `Row` widgets for the visible slice of
/// `filtered_indices`, keeping render cost O(visible) rather than
/// O(total). The `TableState` selection index is then offset by
/// `viewport_start` so the highlight tracks correctly.
fn draw_table(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect, tc: &ThemeColors) {
    let now = Utc::now();
    let selected_offset_secs = now
        .with_timezone(&app.selected_timezone)
        .offset()
        .fix()
        .local_minus_utc();

    let header_cells = [
        "City",
        "Country",
        "Region",
        "Local Time",
        "UTC Offset",
        "Diff",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(tc.accent).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1);

    let visible_rows = (area.height as usize).saturating_sub(3).max(1);
    let total_rows = app.filtered_indices.len();
    let viewport_start = if total_rows <= visible_rows || app.selected_row < visible_rows {
        0
    } else {
        app.selected_row + 1 - visible_rows
    };
    let viewport_end = (viewport_start + visible_rows).min(total_rows);

    let rows: Vec<Row> = app.filtered_indices[viewport_start..viewport_end]
        .iter()
        .zip(app.filtered_display_names[viewport_start..viewport_end].iter())
        .map(|(&idx, &display_name)| {
            let entry = &app.timezones[idx];
            let local = now.with_timezone(&entry.tz);
            let offset_secs = local.offset().fix().local_minus_utc();
            let hour = local.hour();
            let is_day = (6..18).contains(&hour);

            let time_str = local.format("%H:%M:%S").to_string();
            let time_color = if is_day { tc.good } else { tc.muted };

            let utc_offset = format_utc_offset(offset_secs);
            let diff = format_diff(offset_secs, selected_offset_secs);

            let is_fav = app.is_favorite(idx);
            let city_cell = if is_fav {
                Cell::from(Line::from(vec![
                    Span::styled("\u{2605} ", Style::default().fg(tc.star)),
                    Span::styled(display_name, Style::default().fg(tc.fg)),
                ]))
            } else {
                Cell::from(display_name).style(Style::default().fg(tc.fg))
            };

            Row::new(vec![
                city_cell,
                Cell::from(entry.country).style(Style::default().fg(tc.muted)),
                Cell::from(entry.region).style(Style::default().fg(tc.muted)),
                Cell::from(time_str).style(Style::default().fg(time_color)),
                Cell::from(utc_offset).style(Style::default().fg(tc.muted)),
                Cell::from(diff).style(Style::default().fg(tc.info)),
            ])
        })
        .collect();

    let count_text = if app.show_favorites_only {
        format!(
            " {}/{} timezones \u{2605} ",
            app.filtered_indices.len(),
            app.timezones.len()
        )
    } else {
        format!(
            " {}/{} timezones ",
            app.filtered_indices.len(),
            app.timezones.len()
        )
    };

    let widths = [
        Constraint::Percentage(30), // city
        Constraint::Ratio(1, 7),    // country
        Constraint::Ratio(1, 7),    // region
        Constraint::Ratio(1, 7),    // local time
        Constraint::Ratio(1, 7),    // utc offset
        Constraint::Ratio(1, 7),    // diff
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(tc.border))
                .title(count_text)
                .title_style(Style::default().fg(tc.fg)),
        )
        .row_highlight_style(
            Style::default()
                .fg(tc.highlight_fg)
                .bg(tc.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{25b6} ");

    let mut state = TableState::default();
    if !app.filtered_indices.is_empty() {
        state.select(Some(app.selected_row.saturating_sub(viewport_start)));
    }

    frame.render_stateful_widget(table, area, &mut state);

    if app.filtered_indices.len() > (area.height as usize).saturating_sub(3) {
        let mut scrollbar_state =
            ScrollbarState::new(app.filtered_indices.len()).position(app.selected_row);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("\u{2191}"))
                .end_symbol(Some("\u{2193}")),
            area,
            &mut scrollbar_state,
        );
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect, tc: &ThemeColors) {
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

    let hints = match app.input_mode {
        InputMode::Normal => {
            " q:quit  j/k:nav  Enter:select  /:search  f:fav  F:fav-only  J/K:reorder  t:theme  c:copy"
        }
        InputMode::Search => " Esc/Enter:close  Ctrl-u:clear",
    };
    let hint_span = Span::styled(hints, Style::default().fg(tc.muted));

    let show_flash = app
        .copied_flash
        .map(|t| t.elapsed() < std::time::Duration::from_secs(2))
        .unwrap_or(false);

    let mut spans = vec![mode_span, hint_span];
    if show_flash {
        spans.push(Span::styled(
            " Copied!",
            Style::default().fg(tc.good).add_modifier(Modifier::BOLD),
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
/// Returns `"---"` when both offsets are identical (same zone),
/// otherwise `"+Nh"` / `"-N:MM"` etc.
fn format_diff(offset_secs: i32, selected_offset_secs: i32) -> String {
    let diff_secs = offset_secs - selected_offset_secs;
    if diff_secs == 0 {
        return "---".to_string();
    }
    let sign = if diff_secs > 0 { '+' } else { '-' };
    let abs = diff_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    if mins == 0 {
        format!("{}{}h", sign, hours)
    } else {
        format!("{}{}:{:02}", sign, hours, mins)
    }
}
