//! Clock digits drawn from strokes onto an octant canvas.
//!
//! Each glyph is a few polylines in a box one unit tall. Drawing them
//! at the size the hero asks for, instead of scaling a fixed bitmap,
//! keeps the curves smooth at every rung of the hero ladder.

use std::ops::Range;

use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};

/// Every digit shares one width, so the clock does not shift sideways
/// when a 1 becomes an 8.
const DIGIT_WIDTH: f64 = 0.54;
const COLON_WIDTH: f64 = 0.26;
const GAP: f64 = 0.11;
/// Bold at 21 rows, and thin enough at 4 rows that the counters of 4,
/// 6, 8 and 9 stay open.
const STROKE: f64 = 0.14;

type Point = (f64, f64);

/// What a pixel of the clock belongs to, which decides its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    /// A pixel of a digit, `down` its glyph from 0 at the top to 1 at the
    /// bottom. A rolling glyph carries its own `down` as it moves.
    Digit {
        down: f64,
    },
    Colon,
}

/// The width of `text` in cells when drawn `rows` cells tall.
pub fn width(text: &str, rows: u16) -> u16 {
    let pixels = layout(text, rows).last().map_or(0, |&(_, x, w)| x + w);
    u16::try_from(pixels.div_ceil(CELL_WIDTH)).unwrap_or(u16::MAX)
}

/// Paints the change from `from` to `to` at `progress`, from 0 at the
/// start to 1 at the end, onto `canvas` from its top left, `rows` cells
/// tall. Each inked pixel takes the colour `colour` gives its [`Part`].
/// Each glyph of `to` that differs from the glyph of `from` at the same
/// place rolls: the old glyph leaves upward and the new glyph enters from
/// below. Both texts must share one layout, as two times of day do.
pub fn paint(
    canvas: &mut Canvas,
    from: &str,
    to: &str,
    rows: u16,
    progress: f64,
    colour: impl Fn(Part) -> Color,
) {
    let tall = usize::from(rows) * CELL_HEIGHT;
    let scale = tall as f64;
    let shift = progress.clamp(0.0, 1.0);
    let mut leaving = from.chars();
    for (c, x0, w) in layout(to, rows) {
        let old = leaving.next();
        if c == ':' {
            for (xs, ys) in colon_dots(x0, w, tall) {
                for y in ys {
                    for x in xs.clone() {
                        canvas.set(x, y, colour(Part::Colon));
                    }
                }
            }
            continue;
        }
        let entering = strokes(c);
        let left = old.filter(|&old| old != c).map(strokes);
        for y in 0..tall {
            for x in 0..w {
                let p = ((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale);
                let down = match &left {
                    None => entering.iter().any(|line| near(line, p)).then_some(p.1),
                    Some(old) => {
                        let up = (p.0, p.1 + shift);
                        let below = (p.0, up.1 - 1.0);
                        if up.1 < 1.0 && old.iter().any(|line| near(line, up)) {
                            Some(up.1)
                        } else if below.1 >= 0.0 && entering.iter().any(|line| near(line, below)) {
                            Some(below.1)
                        } else {
                            None
                        }
                    }
                };
                if let Some(down) = down {
                    canvas.set(x0 + x, y, colour(Part::Digit { down }));
                }
            }
        }
    }
}

/// The two dots of a colon whose box starts at pixel `x0` and is `w`
/// pixels wide, as (columns, rows) of pixels, for a glyph `tall` pixels
/// tall.
///
/// A round dot lands on partial cells and reads as chipped. So each dot
/// is a square one stroke thick that starts and ends on a cell edge
/// across and on a half cell down, which the full and half blocks draw
/// solid.
fn colon_dots(x0: usize, w: usize, tall: usize) -> [(Range<usize>, Range<usize>); 2] {
    let snap = |at: f64, step: usize| ((at / step as f64).round().max(0.0) as usize) * step;
    let half_cell = CELL_HEIGHT / 2;
    let side = half_cell * ((STROKE * tall as f64 / half_cell as f64).round() as usize).max(1);
    let down_step = if side.is_multiple_of(CELL_HEIGHT) {
        CELL_HEIGHT
    } else {
        half_cell
    };
    let left = snap(x0 as f64 + (w as f64 - side as f64) / 2.0, CELL_WIDTH);
    let dot = |centre: f64| {
        let top = snap(centre * tall as f64 - side as f64 / 2.0, down_step);
        (left..left + side, top..top + side)
    };
    [dot(0.325), dot(0.725)]
}

/// Places each glyph as (char, first pixel, width in pixels).
fn layout(text: &str, rows: u16) -> Vec<(char, usize, usize)> {
    let tall = f64::from(rows) * CELL_HEIGHT as f64;
    let pixels = |units: f64| (units * tall).round() as usize;
    let mut x = 0;
    let mut placed = Vec::new();
    for (i, c) in text.chars().enumerate() {
        if i > 0 {
            x += pixels(GAP);
        }
        let w = pixels(if c == ':' { COLON_WIDTH } else { DIGIT_WIDTH });
        placed.push((c, x, w));
        x += w;
    }
    placed
}

fn near(line: &[Point], p: Point) -> bool {
    line.windows(2)
        .any(|pair| distance_to_segment(p, pair[0], pair[1]) <= STROKE / 2.0)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// An elliptical arc from `from` to `to` degrees. Angles turn clockwise
/// on screen, because y grows downward: 0 is right, 90 is the bottom.
fn arc(centre: Point, rx: f64, ry: f64, from: f64, to: f64) -> Vec<Point> {
    let steps = ((to - from).abs() / 7.5).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let angle = (from + (to - from) * i as f64 / steps as f64).to_radians();
            (centre.0 + rx * angle.cos(), centre.1 + ry * angle.sin())
        })
        .collect()
}

/// The centre lines of each glyph, in a box one unit tall with y
/// downward. Each line is inset by half a stroke, so the ink stays
/// inside the box.
fn strokes(c: char) -> Vec<Vec<Point>> {
    let inset = STROKE / 2.0;
    let (left, right, top, bottom) = (inset, DIGIT_WIDTH - inset, inset, 1.0 - inset);
    let centre_x = DIGIT_WIDTH / 2.0;
    let waist = 0.47;
    let rx = (right - left) / 2.0;
    let upper = (waist - top) / 2.0;
    let lower = (bottom - waist) / 2.0;
    let six = || {
        let bowl = rx * 1.3;
        vec![
            arc(
                (right, bottom - bowl),
                right - left,
                bottom - bowl - top,
                262.0,
                180.0,
            ),
            arc((centre_x, bottom - bowl), rx, bowl, 0.0, 360.0),
        ]
    };
    match c {
        '0' => {
            let cap = 0.24;
            let mut ring = arc((centre_x, top + cap), rx, cap, 180.0, 360.0);
            ring.extend(arc((centre_x, bottom - cap), rx, cap, 0.0, 180.0));
            ring.push((left, top + cap));
            vec![ring]
        }
        '1' => vec![vec![
            (centre_x - 0.17, top + 0.17),
            (centre_x + 0.04, top),
            (centre_x + 0.04, bottom),
        ]],
        '2' => {
            let mut line = arc(
                (centre_x, top + upper * 1.05),
                rx,
                upper * 1.05,
                190.0,
                380.0,
            );
            line.extend([(left, bottom), (right, bottom)]);
            vec![line]
        }
        '3' => vec![
            arc((centre_x, top + upper), rx * 0.92, upper, 205.0, 450.0),
            arc((centre_x, waist + lower), rx, lower, 270.0, 515.0),
        ],
        '4' => vec![vec![
            (right - 0.1, bottom),
            (right - 0.1, top),
            (left, 0.68),
            (right, 0.68),
        ]],
        '5' => {
            let bowl = (bottom - 0.40) / 2.0;
            let mut line = vec![(right, top), (left + 0.03, top), (left, waist)];
            line.extend(arc((centre_x, bottom - bowl), rx, bowl, 215.0, 520.0));
            vec![line]
        }
        '6' => six(),
        '7' => vec![vec![(left, top), (right, top), (centre_x - 0.06, bottom)]],
        '8' => vec![
            arc((centre_x, top + upper), rx * 0.86, upper, 0.0, 360.0),
            arc((centre_x, waist + lower), rx, lower, 0.0, 360.0),
        ],
        '9' => six()
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|(x, y)| (DIGIT_WIDTH - x, 1.0 - y))
                    .collect()
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;

    const INK: Color = Color::Rgb(235, 111, 146);
    const OTHER: Color = Color::Rgb(156, 207, 216);

    /// Paints the roll from `from` to `to` at `progress` with each part in
    /// the colour `colour` gives, and draws it into a buffer of its size.
    fn draw_rolling(
        from: &str,
        to: &str,
        rows: u16,
        progress: f64,
        colour: impl Fn(Part) -> Color,
    ) -> Buffer {
        let area = Rect::new(0, 0, width(to, rows), rows);
        let mut canvas = Canvas::new(area.width, area.height);
        paint(&mut canvas, from, to, rows, progress, colour);
        let mut buffer = Buffer::empty(area);
        canvas.render(area, &mut buffer);
        buffer
    }

    /// The glyphs of a buffer, one string per row.
    fn glyphs(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// The roll from `from` to `to` at `progress` in one ink, one string
    /// per row.
    fn render_rolling(from: &str, to: &str, rows: u16, progress: f64) -> Vec<String> {
        glyphs(&draw_rolling(from, to, rows, progress, |_| INK))
    }

    /// `text` at rest in one ink, one string per row.
    fn render(text: &str, rows: u16) -> Vec<String> {
        render_rolling(text, text, rows, 0.0)
    }

    /// A zero is a closed ring that is empty in the middle.
    ///
    /// Given: the digit zero at the largest hero size.
    /// When: it is drawn.
    /// Then: the middle row has ink at both sides and none at the centre.
    ///
    /// Why it matters: a slash or ink in the centre of each zero makes
    /// a clock full of zeros noisy, and a blocked counter makes 0, 6, 8
    /// and 9 hard to tell apart at a glance.
    #[test]
    fn the_zero_is_a_ring_with_no_slash() {
        // Given:
        let rows = 21;

        // When:
        let art = render("0", rows);

        // Then:
        let middle: Vec<char> = art[usize::from(rows / 2)].chars().collect();
        let centre = middle.len() / 2;
        assert_eq!(
            middle[centre],
            ' ',
            "ink at the centre:\n{}",
            art.join("\n")
        );
        assert_ne!(middle[0], ' ', "no left side:\n{}", art.join("\n"));
        assert_ne!(
            middle[middle.len() - 1],
            ' ',
            "no right side:\n{}",
            art.join("\n")
        );
    }

    /// Every time of day draws at the same width.
    ///
    /// Given: times whose digits differ in shape, from all ones to all
    /// eights.
    /// When: each is drawn at the same size.
    /// Then: every row of every drawing has the width `width` reports.
    ///
    /// Why it matters: the hero centres the art by its width, so a
    /// digit narrower than the others would shift the clock sideways
    /// every second.
    #[test]
    fn every_time_of_day_draws_at_the_same_width() {
        // Given:
        let times = ["11:11:11", "00:00:00", "23:59:47", "88:88:88"];
        let rows = 7;

        // When:
        let drawings: Vec<Vec<String>> = times.iter().map(|t| render(t, rows)).collect();

        // Then:
        let expected = usize::from(width("00:00:00", rows));
        assert!(expected > 0, "width reports no cells");
        for (time, art) in times.iter().zip(&drawings) {
            assert_eq!(art.len(), usize::from(rows), "{time} rows");
            for line in art {
                assert_eq!(
                    line.chars().count(),
                    expected,
                    "{time}:\n{}",
                    art.join("\n")
                );
            }
        }
    }

    /// A roll starts on the old time and ends on the new one.
    ///
    /// Given: a change from 12:34:56 to 12:34:57.
    /// When: the roll is drawn at its start and at its end.
    /// Then: the start is the old time and the end is the new time.
    ///
    /// Why it matters: a roll that jumps at either end makes the clock
    /// flicker once a second.
    #[test]
    fn a_roll_starts_on_the_old_time_and_ends_on_the_new() {
        // Given:
        let (from, to, rows) = ("12:34:56", "12:34:57", 7);

        // When:
        let start = render_rolling(from, to, rows, 0.0);
        let end = render_rolling(from, to, rows, 1.0);

        // Then:
        assert_eq!(start, render(from, rows), "start:\n{}", start.join("\n"));
        assert_eq!(end, render(to, rows), "end:\n{}", end.join("\n"));
    }

    /// The old digit leaves upward and the new digit enters from below.
    ///
    /// Given: a change from 6 to 7 at a height of 8 rows.
    /// When: the roll is drawn half way.
    /// Then: the top 4 rows are the lower half of the 6, and the bottom
    /// 4 rows are the upper half of the 7.
    ///
    /// Why it matters: an odometer rolls one way. A digit that came in
    /// from the top, or that scrolled half a step, would read as a
    /// glitch.
    #[test]
    fn the_old_digit_leaves_upward_and_the_new_digit_enters_from_below() {
        // Given:
        let rows = 8;
        let half = usize::from(rows / 2);

        // When:
        let mid = render_rolling("6", "7", rows, 0.5);

        // Then:
        assert_eq!(mid[..half], render("6", rows)[half..], "top half");
        assert_eq!(mid[half..], render("7", rows)[..half], "bottom half");
    }

    /// Only the digits that change roll.
    ///
    /// Given: a change from 12:34:56 to 12:34:57.
    /// When: the roll is drawn half way.
    /// Then: the cells of 12:34:5 are the same as at rest, and the
    /// drawing is neither the old nor the new time.
    ///
    /// Why it matters: a clock that rolls every digit each second is
    /// noise. Only the seconds digit changes most of the time.
    #[test]
    fn only_the_digits_that_change_roll() {
        // Given:
        let (from, to, rows) = ("12:34:56", "12:34:57", 7);
        let still = usize::from(width("12:34:5", rows));

        // When:
        let mid = render_rolling(from, to, rows, 0.5);

        // Then:
        let at_rest = render(to, rows);
        for (row, rest) in mid.iter().zip(&at_rest) {
            let prefix: String = row.chars().take(still).collect();
            let rest_prefix: String = rest.chars().take(still).collect();
            assert_eq!(prefix, rest_prefix, "mid:\n{}", mid.join("\n"));
        }
        assert_ne!(mid, render(from, rows), "the roll did not move");
        assert_ne!(mid, at_rest, "the roll did not move");
    }

    /// Each colon dot is a solid square at every clock size.
    ///
    /// Given: each rung of the hero ladder.
    /// When: a time is drawn.
    /// Then: the first colon is two dots, each a solid rectangle of full
    /// or half blocks that is as many dots wide as it is tall.
    ///
    /// Why it matters: a dot drawn from a round stroke lands on partial
    /// cells, and reads as chipped next to the solid digit strokes.
    #[test]
    fn each_colon_dot_is_a_solid_square_at_every_size() {
        for rows in [4, 7, 14, 21] {
            // Given:
            let colour = |part| match part {
                Part::Colon => OTHER,
                Part::Digit { .. } => INK,
            };

            // When:
            let buffer = draw_rolling("00:00:00", "00:00:00", rows, 0.0, colour);
            let art = glyphs(&buffer);

            // Then:
            let shown = art.join("\n");
            let first = (0..buffer.area.width)
                .find(|&x| (0..rows).any(|y| buffer[(x, y)].fg == OTHER))
                .unwrap_or_else(|| panic!("rows {rows}: no colon:\n{shown}"));
            let end = (first..buffer.area.width)
                .find(|&x| (0..rows).all(|y| buffer[(x, y)].fg != OTHER))
                .unwrap_or(buffer.area.width);
            let colon = usize::from(first)..usize::from(end);
            let mut halves: Vec<Option<String>> = Vec::new();
            for row in &art {
                let cells: String = row.chars().skip(colon.start).take(colon.len()).collect();
                let ink = cells.trim();
                let (top, bottom) = match ink.chars().next() {
                    None => (false, false),
                    Some('\u{2588}') => (true, true),
                    Some('\u{2580}') => (true, false),
                    Some('\u{2584}') => (false, true),
                    Some(_) => panic!("rows {rows}: chipped:\n{shown}"),
                };
                assert!(
                    ink.chars().all(|c| ink.starts_with(c)),
                    "rows {rows}: chipped:\n{shown}"
                );
                let span = (!ink.is_empty()).then(|| cells.clone());
                halves.push(span.clone().filter(|_| top));
                halves.push(span.filter(|_| bottom));
            }
            let mut dots: Vec<Vec<String>> = Vec::new();
            let mut previous_inked = false;
            for half in halves {
                let inked = half.is_some();
                match (half, previous_inked) {
                    (Some(cells), true) => dots.last_mut().unwrap().push(cells),
                    (Some(cells), false) => dots.push(vec![cells]),
                    (None, _) => {}
                }
                previous_inked = inked;
            }
            assert_eq!(dots.len(), 2, "rows {rows}:\n{shown}");
            for dot in &dots {
                let width = dot[0].trim().chars().count() * CELL_WIDTH;
                let height = dot.len() * CELL_HEIGHT / 2;
                assert!(
                    dot.iter().all(|cells| cells == &dot[0]),
                    "rows {rows}: not a rectangle:\n{shown}"
                );
                assert_eq!(width, height, "rows {rows}: not square:\n{shown}");
            }
        }
    }

    /// Digits and colons never share a cell.
    ///
    /// Given: the widest time of day, with the digits in one ink and the
    /// colons in another, at each rung of the hero ladder.
    /// When: it is drawn.
    /// Then: every cell with ink has the terminal's own background behind
    /// it, so no cell holds both inks.
    ///
    /// Why it matters: the colons pulse apart from the digits. A cell that
    /// held both would take one of the two inks for all its pixels.
    #[test]
    fn digits_and_colons_never_share_a_cell() {
        for rows in [4, 7, 14, 21] {
            // Given:
            let colour = |part| match part {
                Part::Colon => OTHER,
                Part::Digit { .. } => INK,
            };

            // When:
            let buffer = draw_rolling("88:88:88", "88:88:88", rows, 0.0, colour);

            // Then:
            for cell in buffer.content() {
                if cell.symbol() != " " {
                    assert_eq!(cell.bg, Color::Reset, "rows {rows}: {cell:?}");
                }
            }
        }
    }

    /// A rolling digit carries its own shade.
    ///
    /// Given: a change from 6 to 7 at a height of 8 rows, with each digit
    /// pixel in one ink in the lower half of its glyph and in another ink
    /// in the upper half.
    /// When: the roll is drawn half way.
    /// Then: the top 4 rows show only the lower-half ink, and the bottom 4
    /// rows show only the upper-half ink.
    ///
    /// Why it matters: the clock shades each digit from top to bottom. A
    /// shade fixed to the screen would slide through a digit as it rolls.
    #[test]
    fn a_rolling_digit_carries_its_own_shade() {
        // Given:
        let rows = 8;
        let colour = |part| match part {
            Part::Digit { down } if down >= 0.5 => INK,
            _ => OTHER,
        };

        // When:
        let buffer = draw_rolling("6", "7", rows, 0.5, colour);

        // Then:
        for y in 0..rows {
            let expected = if y < rows / 2 { INK } else { OTHER };
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol() != " " {
                    assert_eq!(cell.fg, expected, "({x}, {y})");
                }
            }
        }
    }
}
