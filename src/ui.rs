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
use chrono::{DateTime, Utc};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState,
    },
};
use std::borrow::Cow;

use tui_big_text::{BigText, PixelSize};

use unicode_width::UnicodeWidthStr;

use crate::app::{App, CopyStatus, InputMode};
use crate::theme::ThemeColors;
use crate::timezone::{format_utc_offset, is_daytime_at};

/// Rows of the table area consumed by non-data chrome: top border,
/// header row, bottom border. Used to size the viewport and decide
/// whether the scrollbar is needed.
const TABLE_CHROME_ROWS: usize = 3;

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
            Constraint::Length(7), // big clock
            Constraint::Length(3), // search bar
            Constraint::Min(8),    // timezone table
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    draw_title_bar(frame, app, outer[0], &tc);
    draw_big_clock(frame, app, &now, outer[1], &tc);
    draw_search_bar(frame, app, outer[2], &tc);
    draw_table(frame, app, &now, outer[3], &tc);
    draw_status_bar(frame, app, outer[4], &tc);

    if app.show_help {
        draw_help_popup(frame, frame.area(), &tc);
    }
}

/// Renders a centered help popup listing every keybinding, grouped
/// by mode. Dismissed by any keypress (handled in `events.rs`).
///
/// Uses [`Clear`] to wipe the underlying cells before drawing, so
/// the popup's transparent borders don't bleed background through.
fn draw_help_popup(frame: &mut Frame, area: Rect, tc: &ThemeColors) {
    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Normal mode",
            Style::default().fg(tc.accent).add_modifier(Modifier::BOLD),
        )),
        help_row("j / k / ↑ ↓", "Move up / down", tc),
        help_row("g / G", "Jump to top / bottom", tc),
        help_row("Ctrl-d / Ctrl-u", "Page down / up", tc),
        help_row("Enter", "Select timezone (set as main clock)", tc),
        help_row("/", "Enter search mode", tc),
        help_row("f", "Toggle favorite on selected row", tc),
        help_row("F", "Toggle favorites-only filter", tc),
        help_row("J / K", "Move favorite down / up in order", tc),
        help_row("t", "Cycle theme", tc),
        help_row("c", "Copy selected time to clipboard", tc),
        help_row("?", "Toggle this help", tc),
        help_row("q / Ctrl-c", "Quit", tc),
        Line::from(""),
        Line::from(Span::styled(
            "Search mode",
            Style::default().fg(tc.accent).add_modifier(Modifier::BOLD),
        )),
        help_row("Type", "Filter timezones (AND across terms)", tc),
        help_row("← / →", "Move cursor", tc),
        help_row(
            "Home / End",
            "Jump to start / end (also Ctrl-a / Ctrl-e)",
            tc,
        ),
        help_row("Backspace / Delete", "Delete previous / next char", tc),
        help_row("Ctrl-u", "Clear search", tc),
        help_row("Esc / Enter", "Exit search", tc),
        Line::from(""),
        Line::from(Span::styled(
            "Search syntax",
            Style::default().fg(tc.accent).add_modifier(Modifier::BOLD),
        )),
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
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to dismiss",
            Style::default().fg(tc.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let popup_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let popup_width: u16 = 64;
    let popup = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tc.accent))
        .title(" Help ")
        .title_style(Style::default().fg(tc.title).add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(lines)
        .block(block)
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
/// Sized to fit the 64-col help popup (~16 + 2 + 38 ~= 58 payload cols).
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
fn draw_big_clock(
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

    if area.width < 60 {
        let lines = vec![
            Line::from(Span::styled(
                format!("  {}", time_str),
                Style::default()
                    .fg(clock_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  {} - {}", app.selection.city_name, date_str),
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
        .filter(|(tz, _)| *tz != app.selection.tz)
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

    let clock_area = Rect {
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
        format!("  {} - {}", app.selection.city_name, date_str),
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
            draw_side_clock(frame, utc_now, tz, city, side_chunks[i], tc);
        }
    }
}

/// Renders a single favourite side clock: city label, Quadrant-pixel
/// time, and abbreviated date, separated by a vertical border line.
fn draw_side_clock(
    frame: &mut Frame,
    utc_now: &DateTime<Utc>,
    tz: chrono_tz::Tz,
    city: &str,
    area: Rect,
    tc: &ThemeColors,
) {
    let local = utc_now.with_timezone(&tz);
    let is_day = is_daytime_at(tz, &local);
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
    let clock_inner = Rect {
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

fn draw_search_bar(frame: &mut Frame, app: &App, area: Rect, tc: &ThemeColors) {
    let border_color = match app.input_mode {
        InputMode::Search => tc.accent,
        InputMode::Normal => tc.border,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Search ")
        .title_style(Style::default().fg(tc.title));

    // Inner width is `area.width - 2` (one column for each border).
    let inner_width = area.width.saturating_sub(2);

    // Compute horizontal scroll so the cursor stays visible. The
    // cursor column is the display width of the query slice up to
    // `cursor_position`. If it exceeds inner_width, we scroll right
    // by `cursor_col - inner_width + 1` so the cursor sits one column
    // inside the right border.
    let cursor_col = app.search_query[..app.cursor_position].width() as u16;
    let scroll_x = cursor_col.saturating_sub(inner_width.saturating_sub(1));

    // Three states drive the input contents:
    //  * Normal mode + empty query   → muted "type / to search..." prompt.
    //  * Search mode + empty query   → muted syntax hint sitting AFTER the
    //                                  cursor (which still anchors at col 0).
    //                                  Hint disappears the moment a character
    //                                  is typed because we take the
    //                                  non-empty-query branch below.
    //  * any query                   → render the query in `fg` colour.
    let line: Line = if app.search_query.is_empty() {
        if app.input_mode == InputMode::Search {
            Line::from(Span::styled(
                "city, country, +5:30",
                Style::default().fg(tc.muted),
            ))
        } else {
            Line::from(Span::styled(
                "type / to search...",
                Style::default().fg(tc.muted),
            ))
        }
    } else {
        Line::from(Span::styled(&app.search_query, Style::default().fg(tc.fg)))
    };

    let p = Paragraph::new(line)
        .block(block)
        .style(Style::default().bg(tc.bg))
        .scroll((0, scroll_x));
    frame.render_widget(p, area);

    if app.input_mode == InputMode::Search {
        let visible_col = cursor_col.saturating_sub(scroll_x);
        // Clamp to inside the right border so we never render past it.
        let max_x = area.x + area.width.saturating_sub(1);
        let x = (area.x + 1 + visible_col).min(max_x);
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
fn draw_table(frame: &mut Frame, app: &mut App, now: &DateTime<Utc>, area: Rect, tc: &ThemeColors) {
    // Empty-results guidance: when the user has typed a query that
    // matches nothing, replace the empty table body with a centred
    // hint pointing at the syntax they probably haven't discovered.
    // Title still reads "0/N timezones" so the count is unambiguous.
    if app.filtered_view.is_empty() && !app.search_query.is_empty() {
        let count_text = format!(
            " {}/{} timezones ",
            app.filtered_view.len(),
            app.catalogue.len()
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(tc.border))
            .title(count_text)
            .title_style(Style::default().fg(tc.fg));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Truncate the displayed query to ~24 display cols (unicode-width aware).
        let truncated = truncate_display(&app.search_query, 24);

        // Vertically centre the hint inside the table body.
        let pad_top = inner.height.saturating_sub(1) / 2;
        let hint_area = Rect {
            x: inner.x,
            y: inner.y + pad_top,
            width: inner.width,
            height: 1.min(inner.height),
        };
        let msg = format!(
            "No matches for \"{}\". Try a city, country, or +5:30.",
            truncated
        );
        let p = Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(tc.muted))))
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(tc.bg));
        frame.render_widget(p, hint_area);
        return;
    }

    let selected_offset_secs = now
        .with_timezone(&app.selection.tz)
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

    let visible_rows = (area.height as usize)
        .saturating_sub(TABLE_CHROME_ROWS)
        .max(1);
    let total_rows = app.filtered_view.len();
    let viewport_start = if total_rows <= visible_rows || app.selected_row < visible_rows {
        0
    } else {
        app.selected_row + 1 - visible_rows
    };
    let viewport_end = (viewport_start + visible_rows).min(total_rows);

    // Loop-invariant styles — `Style` is `Copy`, so hoisting avoids
    // rebuilding identical structs once per visible row per frame.
    let muted_style = Style::default().fg(tc.muted);
    let info_style = Style::default().fg(tc.info);
    let fg_style = Style::default().fg(tc.fg);
    let star_style = Style::default().fg(tc.star);
    // Day/night time colours: both branches of the per-row ternary
    // collapse to one of these two styles, so build them once outside
    // the loop instead of materialising a fresh `Style` per row.
    let day_time_style = Style::default().fg(tc.good);
    let night_time_style = Style::default().fg(tc.muted);

    // The search query is invariant across every visible row, so
    // lowercase it once per frame instead of inside `city_name_spans`
    // for every row. Empty result means "no highlight" downstream.
    let needle_lc = app.search_query.trim().to_lowercase();
    let needle_lc: &str = needle_lc.as_str();

    let rows: Vec<Row> = app.filtered_view.rows()[viewport_start..viewport_end]
        .iter()
        .map(|row| {
            let idx = row.catalogue_idx;
            let display_name = row.display_name;
            // `filtered_view` is constructed exclusively from
            // `0..catalogue.len()` indices in `App::set_sorted_results`
            // and is invalidated on every catalogue change, so this
            // lookup cannot return `None` in well-formed states. The
            // `expect` documents the invariant rather than papering
            // over a fallible call.
            #[allow(
                clippy::expect_used,
                reason = "filtered_view indices are derived from the catalogue itself; see set_sorted_results"
            )]
            let entry = app
                .catalogue
                .get(idx)
                .expect("filtered_view row references a valid catalogue index");
            let local = now.with_timezone(&entry.tz);
            let offset_secs = local.offset().fix().local_minus_utc();
            let is_day = is_daytime_at(entry.tz, &local);

            let time_str = local.format("%H:%M:%S").to_string();
            let time_style = if is_day {
                day_time_style
            } else {
                night_time_style
            };

            let utc_offset = format_utc_offset(offset_secs);
            let diff: Cow<'static, str> = if entry.tz == app.selection.tz {
                Cow::Borrowed("---")
            } else {
                format_diff(offset_secs, selected_offset_secs)
            };

            let is_fav = app.is_favorite(idx);
            // City name spans: when search is active, dim the non-matching
            // portion of the display name so the matched substring pops.
            // Only the first occurrence of the WHOLE query string is
            // highlighted — multi-term AND queries and per-term highlighting
            // are intentionally out of scope (would require a helper in
            // search.rs, which is out of bounds for this edit).
            let name_spans = city_name_spans(display_name, needle_lc, tc, fg_style);
            let city_cell = if is_fav {
                let mut spans = Vec::with_capacity(name_spans.len() + 1);
                spans.push(Span::styled("\u{2605} ", star_style));
                spans.extend(name_spans);
                Cell::from(Line::from(spans))
            } else {
                Cell::from(Line::from(name_spans))
            };

            Row::new(vec![
                city_cell,
                Cell::from(entry.country).style(muted_style),
                Cell::from(entry.region).style(muted_style),
                Cell::from(time_str).style(time_style),
                Cell::from(utc_offset).style(muted_style),
                Cell::from(diff).style(info_style),
            ])
        })
        .collect();

    let count_text = if app.show_favorites_only {
        format!(
            " {}/{} timezones \u{2605} ",
            app.filtered_view.len(),
            app.catalogue.len()
        )
    } else {
        format!(
            " {}/{} timezones ",
            app.filtered_view.len(),
            app.catalogue.len()
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
    if !app.filtered_view.is_empty() {
        state.select(Some(app.selected_row.saturating_sub(viewport_start)));
    }

    frame.render_stateful_widget(table, area, &mut state);

    if app.filtered_view.len() > (area.height as usize).saturating_sub(TABLE_CHROME_ROWS) {
        let mut scrollbar_state =
            ScrollbarState::new(app.filtered_view.len()).position(app.selected_row);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("\u{2191}"))
                .end_symbol(Some("\u{2193}")),
            area,
            &mut scrollbar_state,
        );
    }
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
        InputMode::Normal => " q:quit  /:search  f:fav  t:theme  c:copy  ?:help",
        InputMode::Search => {
            " Esc/Enter:close  Ctrl-u:clear  hint: city \u{00b7} +5:30 \u{00b7} asia"
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

/// Splits a city display name into `Span`s so the first case-insensitive
/// occurrence of `needle_lc` renders in `fg_style` while the surrounding
/// text dims to `muted`. Pure visual affordance — when the needle is
/// empty, or not found, returns a single `fg_style` span (the prior
/// behaviour).
///
/// `needle_lc` is the **already-trimmed, already-lowercased** search
/// query — the caller hoists the `trim().to_lowercase()` out of the
/// per-row hot loop because the query is invariant across rows in a
/// single frame.
///
/// The search engine matches more broadly than substring (aliases,
/// offsets, regions), so a substring miss is expected — the row still
/// shows up, just without highlight. That's fine: this is cosmetic only.
fn city_name_spans<'a>(
    display_name: &'a str,
    needle_lc: &str,
    tc: &ThemeColors,
    fg_style: Style,
) -> Vec<Span<'a>> {
    if needle_lc.is_empty() {
        return vec![Span::styled(display_name, fg_style)];
    }
    let haystack_lc = display_name.to_lowercase();
    let Some(start) = haystack_lc.find(needle_lc) else {
        return vec![Span::styled(display_name, fg_style)];
    };
    // `to_lowercase` can change byte length (e.g. ß → ss), making indices
    // from the lowercased string unsafe against the original. Fall back to
    // the un-highlighted span when the haystack changed length under
    // case-folding so we never split mid-UTF-8.
    if haystack_lc.len() != display_name.len() {
        return vec![Span::styled(display_name, fg_style)];
    }
    let end = start + needle_lc.len();
    if !display_name.is_char_boundary(start) || !display_name.is_char_boundary(end) {
        return vec![Span::styled(display_name, fg_style)];
    }
    let muted = Style::default().fg(tc.muted);
    let mut spans = Vec::with_capacity(3);
    if start > 0 {
        spans.push(Span::styled(&display_name[..start], muted));
    }
    spans.push(Span::styled(&display_name[start..end], fg_style));
    if end < display_name.len() {
        spans.push(Span::styled(&display_name[end..], muted));
    }
    spans
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
