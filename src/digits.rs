//! Clock digits drawn from strokes into octant cells.
//!
//! Each glyph is a few polylines in a box one unit tall. Drawing them
//! at the size the hero asks for, instead of scaling a fixed bitmap,
//! keeps the curves smooth at every rung of the hero ladder.

use ratatui::symbols::pixel::OCTANTS;

/// An octant cell holds 2 by 4 dots. A terminal cell is about twice as
/// tall as it is wide, so each dot is close to square.
const DOTS_ACROSS: usize = 2;
const DOTS_DOWN: usize = 4;

/// Every digit shares one width, so the clock does not shift sideways
/// when a 1 becomes an 8.
const DIGIT_WIDTH: f64 = 0.54;
const COLON_WIDTH: f64 = 0.26;
const GAP: f64 = 0.11;
/// Bold at 21 rows, and thin enough at 4 rows that the counters of 4,
/// 6, 8 and 9 stay open.
const STROKE: f64 = 0.14;

type Point = (f64, f64);

/// The width of `text` in cells when drawn `rows` cells tall.
pub fn width(text: &str, rows: u16) -> u16 {
    let dots = layout(text, rows).last().map_or(0, |&(_, x, w)| x + w);
    u16::try_from(dots.div_ceil(DOTS_ACROSS)).unwrap_or(u16::MAX)
}

/// Draws `text` as one string per cell row, `rows` rows tall. Every
/// row has [`width`] cells.
pub fn render(text: &str, rows: u16) -> Vec<String> {
    let tall = usize::from(rows) * DOTS_DOWN;
    let glyphs = layout(text, rows);
    let wide = glyphs.last().map_or(0, |&(_, x, w)| x + w);
    let scale = tall as f64;

    let mut ink = vec![false; wide * tall];
    for &(c, x0, w) in &glyphs {
        let lines = strokes(c);
        for y in 0..tall {
            for x in 0..w {
                let p = ((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale);
                ink[y * wide + x0 + x] = lines.iter().any(|line| near(line, p));
            }
        }
    }

    let inked = |x: usize, y: usize| x < wide && ink[y * wide + x];
    (0..tall)
        .step_by(DOTS_DOWN)
        .map(|y0| {
            (0..wide)
                .step_by(DOTS_ACROSS)
                .map(|x0| {
                    let mut bits = 0;
                    for dy in 0..DOTS_DOWN {
                        for dx in 0..DOTS_ACROSS {
                            if inked(x0 + dx, y0 + dy) {
                                bits |= 1 << (dy * DOTS_ACROSS + dx);
                            }
                        }
                    }
                    OCTANTS[bits]
                })
                .collect()
        })
        .collect()
}

/// Places each glyph as (char, first dot, width in dots).
fn layout(text: &str, rows: u16) -> Vec<(char, usize, usize)> {
    let tall = f64::from(rows) * DOTS_DOWN as f64;
    let dots = |units: f64| (units * tall).round() as usize;
    let mut x = 0;
    let mut placed = Vec::new();
    for (i, c) in text.chars().enumerate() {
        if i > 0 {
            x += dots(GAP);
        }
        let w = dots(if c == ':' { COLON_WIDTH } else { DIGIT_WIDTH });
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
        ':' => {
            let x = COLON_WIDTH / 2.0;
            vec![vec![(x, 0.30), (x, 0.35)], vec![(x, 0.70), (x, 0.75)]]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

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
}
