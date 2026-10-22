//! Eight pixels in one cell, two across and four down, drawn as an octant
//! glyph and two colours, and colour mixing in OKLab, for shades that
//! change in even steps to the eye.
//!
//! The canvas draws over the terminal's own background, whose colour the
//! app cannot know. So a cell keeps that background behind its ink, and
//! no colour ever mixes with it.

use std::cmp::Reverse;
use std::ops::Range;
use std::sync::LazyLock;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use ratatui::symbols::pixel::OCTANTS;

/// An octant cell is two pixels wide and four tall.
pub const CELL_WIDTH: usize = 2;
pub const CELL_HEIGHT: usize = 4;

/// The terminal's own background, which every pixel starts on.
const BACKGROUND: Color = Color::Reset;

/// A colour in OKLab, Björn Ottosson's space where an even step in each
/// coordinate looks an even step to the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lab([f32; 3]);

/// Each 8 bit sRGB channel as linear light, so a colour reads each
/// channel with a table read and not a power.
static LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|channel| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    })
});

fn encode(linear: f32) -> u8 {
    let value = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Lab {
    /// The colour in OKLab, or none for a colour with no RGB value, such as
    /// the terminal's own background.
    fn of(colour: Color) -> Option<Self> {
        let Color::Rgb(r, g, b) = colour else {
            return None;
        };
        let [r, g, b] = [r, g, b].map(|channel| LINEAR[usize::from(channel)]);
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        Some(Self([
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]))
    }

    fn rgb(self) -> Color {
        let Self([lightness, a, b]) = self;
        let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        Color::Rgb(
            encode(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            encode(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            encode(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
        )
    }

    fn towards(self, other: Self, share: f32) -> Self {
        Self(std::array::from_fn(|axis| {
            self.0[axis] + (other.0[axis] - self.0[axis]) * share
        }))
    }
}

/// The colours a render has read in OKLab, so a frame takes the cube roots
/// of each colour once and not once for each cell. A colour whose slot
/// another holds is read again, so the cache needs no heap.
#[derive(Debug)]
struct Labs([Option<(Color, Lab)>; 256]);

impl Labs {
    fn new() -> Self {
        Self([None; 256])
    }

    fn of(&mut self, colour: Color) -> Option<Lab> {
        let Color::Rgb(r, g, b) = colour else {
            return None;
        };
        let slot = usize::from(r ^ g.rotate_left(3) ^ b.rotate_left(5));
        match self.0[slot] {
            Some((held, lab)) if held == colour => Some(lab),
            _ => {
                let lab = Lab::of(colour)?;
                self.0[slot] = Some((colour, lab));
                Some(lab)
            }
        }
    }
}

/// The colour `share` of the way from `from` to `to` in OKLab, so each step
/// of a shade looks as large as the last. A share of 0 or less gives `from`
/// and 1 or more gives `to`, each exactly. A colour with no RGB value, such
/// as `Color::Reset`, has nothing to mix with, so the mix is the nearer end:
/// `from` below a half and `to` from a half on.
pub fn mix(from: Color, to: Color, share: f32) -> Color {
    if share <= 0.0 {
        return from;
    }
    if share >= 1.0 {
        return to;
    }
    match (Lab::of(from), Lab::of(to)) {
        (Some(near), Some(far)) => near.towards(far, share).rgb(),
        _ if share < 0.5 => from,
        _ => to,
    }
}

/// The colour `over` gives a pixel of `under` at `share`. Where either is
/// the background, the colour is the nearer end and never a mix.
fn blend_colours(under: Color, over: Color, share: f32) -> Color {
    if under == BACKGROUND || over == BACKGROUND {
        if share >= 0.5 { over } else { under }
    } else {
        mix(under, over, share)
    }
}

/// Each colour of `pixels` once, in the order of its first pixel, with the
/// count of its pixels, and how many colours there are.
fn tally(pixels: [Color; 8]) -> ([(Color, u8); 8], usize) {
    let mut counts = [(Color::Reset, 0); 8];
    let mut colours = 0;
    for colour in pixels {
        match counts[..colours]
            .iter_mut()
            .find(|(each, _)| *each == colour)
        {
            Some((_, count)) => *count += 1,
            None => {
                counts[colours] = (colour, 1);
                colours += 1;
            }
        }
    }
    (counts, colours)
}

/// `pixels` in two colours of `counts`, the colours of the cell: the pair
/// whose total OKLab distance from every pixel to the nearer of the two is
/// least, with each pixel in the nearer. Of two pairs with the same total,
/// the pair whose colours come first wins, and a pixel as near both takes
/// the first, so an instant draws the same frame each time. A colour with
/// no RGB value lies infinitely far from every other.
fn nearest_pair(pixels: [Color; 8], counts: &[(Color, u8)], cache: &mut Labs) -> [Color; 8] {
    let labs: [Option<Lab>; 8] =
        std::array::from_fn(|index| counts.get(index).and_then(|(colour, _)| cache.of(*colour)));
    // Row `one` holds what each colour of the cell costs to draw in colour
    // `one`: its distance to it times its pixels, and 0 past the colours
    // the cell holds. A pair then costs the smaller of its two rows in each
    // place, so the search over pairs needs no multiplication.
    let mut costs = [[0.0_f32; 8]; 8];
    for one in 0..counts.len() {
        for other in one + 1..counts.len() {
            let gap = match (labs[one], labs[other]) {
                (Some(Lab(a)), Some(Lab(b))) => {
                    let squares: f32 = (0..3).map(|axis| (a[axis] - b[axis]).powi(2)).sum();
                    squares.sqrt()
                }
                _ => f32::INFINITY,
            };
            costs[one][other] = f32::from(counts[other].1) * gap;
            costs[other][one] = f32::from(counts[one].1) * gap;
        }
    }
    let mut best = (0, 1);
    let mut least = f32::INFINITY;
    for one in 0..counts.len() {
        for other in one + 1..counts.len() {
            let total: f32 = (0..8)
                .map(|each| costs[one][each].min(costs[other][each]))
                .sum();
            if total < least {
                (best, least) = ((one, other), total);
            }
        }
    }
    let (one, other) = best;
    pixels.map(|colour| {
        let each = counts
            .iter()
            .position(|(held, _)| *held == colour)
            .unwrap_or(one);
        if costs[one][each] <= costs[other][each] {
            counts[one].0
        } else {
            counts[other].0
        }
    })
}

/// The glyph, foreground and background for the pixels of one cell, in the
/// order of the glyph's bits. A glyph holds two colours. A cell of more
/// first takes the pair of its colours that lies nearest its pixels, with
/// each pixel in the nearer of the two. Of the two colours, the more common
/// goes in front, but the background stays behind, so the terminal shows
/// through. Of two colours with the same count, the one with the first
/// pixel goes first, so an instant draws the same frame each time. A cell
/// of one ink is a full block in front of the background.
fn cell(pixels: [Color; 8], cache: &mut Labs) -> (char, Color, Color) {
    let (mut counts, mut colours) = tally(pixels);
    let pixels = if colours > 2 {
        let paired = nearest_pair(pixels, &counts[..colours], cache);
        (counts, colours) = tally(paired);
        paired
    } else {
        pixels
    };
    let counts = &mut counts[..colours];
    counts.sort_by_key(|(_, count)| Reverse(*count));
    let first = counts.first().map_or(BACKGROUND, |(colour, _)| *colour);
    let Some(&(second, _)) = counts.get(1) else {
        return if first == BACKGROUND {
            (OCTANTS[0], BACKGROUND, BACKGROUND)
        } else {
            (OCTANTS[usize::from(u8::MAX)], first, BACKGROUND)
        };
    };
    let (fore, back) = if first == BACKGROUND {
        (second, first)
    } else {
        (first, second)
    };
    let bits = pixels
        .iter()
        .enumerate()
        .filter(|(_, colour)| **colour != back)
        .fold(0, |bits, (pixel, _)| bits | 1 << pixel);
    (OCTANTS[bits], fore, back)
}

/// The points across each side of a pixel that sample its coverage. Four
/// give 17 steps of coverage, enough that the step between two is not seen
/// in a mix of two theme colours.
const SAMPLES: usize = 4;

/// Where each sample lies from the centre of its pixel, across or down.
const OFFSETS: [f32; SAMPLES] = [-0.375, -0.125, 0.125, 0.375];

/// The furthest a sample lies from the centre of its pixel.
const REACH: f32 = 0.375 * std::f32::consts::SQRT_2;

/// Every pixel along one side that a shape from `low` to `high` can touch,
/// cut to the `count` pixels of the canvas, so a shape past an edge draws
/// only the part inside.
fn span(low: f32, high: f32, count: usize) -> Range<usize> {
    let start = (low - 1.0).floor().max(0.0) as usize;
    let end = (high + 2.0).ceil().max(0.0) as usize;
    start.min(count)..end.min(count)
}

/// The pixels of a frame, `CELL_WIDTH` across and `CELL_HEIGHT` down in
/// each cell, that draw into a buffer one glyph to a cell.
#[derive(Debug, Clone)]
pub struct Canvas {
    columns: u16,
    rows: u16,
    pixels: Vec<Color>,
}

impl Canvas {
    /// A canvas of `columns` by `rows` cells with every pixel on the
    /// terminal's own background.
    pub fn new(columns: u16, rows: u16) -> Self {
        let size = usize::from(columns) * CELL_WIDTH * usize::from(rows) * CELL_HEIGHT;
        Self {
            columns,
            rows,
            pixels: vec![BACKGROUND; size],
        }
    }

    /// The pixels across.
    pub fn width(&self) -> usize {
        usize::from(self.columns) * CELL_WIDTH
    }

    /// The pixels down.
    pub fn height(&self) -> usize {
        usize::from(self.rows) * CELL_HEIGHT
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width() && y < self.height()).then(|| y * self.width() + x)
    }

    /// The colour of the pixel at (x, y), or none past an edge. Only a test
    /// reads a pixel, before the cell of two colours reduces it.
    #[cfg(test)]
    pub fn get(&self, x: usize, y: usize) -> Option<Color> {
        self.index(x, y).map(|index| self.pixels[index])
    }

    /// Paints the pixel at (x, y). A pixel past an edge is dropped, so a
    /// shape can cross the edge of the frame.
    pub fn set(&mut self, x: usize, y: usize, colour: Color) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = colour;
        }
    }

    /// Mixes `colour` into the pixel at (x, y) by `coverage`, the share of
    /// the pixel a shape covers, from 0 to 1. Where the pixel or `colour`
    /// is the background, the pixel takes `colour` from a coverage of a
    /// half on and keeps its own colour below, so the edge stays hard. A
    /// pixel past an edge is dropped.
    pub fn blend(&mut self, x: usize, y: usize, colour: Color, coverage: f32) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = blend_colours(self.pixels[index], colour, coverage);
        }
    }

    /// Fills the disc of `radius` round `centre`. As for every shape, a
    /// point is in pixels, and the centre of the pixel at (x, y) lies at
    /// (x, y), so a shape on whole numbers centres on whole pixels.
    pub fn disc(&mut self, centre: (f32, f32), radius: f32, colour: Color) {
        let (x, y) = centre;
        let bounds = [x - radius, y - radius, x + radius, y + radius];
        self.fill(bounds, colour, |px, py| (px - x).hypot(py - y) - radius);
    }

    /// Blends `colour` into each pixel by its coverage of the shape whose
    /// signed distance is `edge`: how far a point lies outside the shape,
    /// and below 0 inside. `bounds` holds the shape: left, top, right and
    /// bottom.
    ///
    /// The coverage of a pixel is the share of `SAMPLES` by `SAMPLES`
    /// points evenly spread over it that lie inside. Each shape takes the
    /// same points, so a frame draws the same each time. A pixel whose
    /// centre lies further than `REACH` from the edge has every point on
    /// one side, so it needs one distance and no samples.
    fn fill(&mut self, bounds: [f32; 4], colour: Color, edge: impl Fn(f32, f32) -> f32) {
        let [left, top, right, bottom] = bounds;
        for y in span(top, bottom, self.height()) {
            for x in span(left, right, self.width()) {
                let (cx, cy) = (x as f32, y as f32);
                let centre = edge(cx, cy);
                let coverage = if centre <= -REACH {
                    1.0
                } else if centre >= REACH {
                    continue;
                } else {
                    let inside = OFFSETS
                        .iter()
                        .flat_map(|dy| OFFSETS.iter().map(move |dx| (dx, dy)))
                        .filter(|(dx, dy)| edge(cx + *dx, cy + *dy) < 0.0)
                        .count();
                    inside as f32 / (SAMPLES * SAMPLES) as f32
                };
                self.blend(x, y, colour, coverage);
            }
        }
    }

    /// Draws each cell into `buf` from the top left of `area`. Cells past
    /// the area are not drawn.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut cache = Labs::new();
        for row in 0..self.rows.min(area.height) {
            for column in 0..self.columns.min(area.width) {
                let (left, top) = (
                    usize::from(column) * CELL_WIDTH,
                    usize::from(row) * CELL_HEIGHT,
                );
                let pixels = std::array::from_fn(|pixel| {
                    let (x, y) = (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
                    self.pixels[y * self.width() + x]
                });
                let (glyph, fore, back) = cell(pixels, &mut cache);
                buf[Position::new(area.x + column, area.y + row)]
                    .set_char(glyph)
                    .set_fg(fore)
                    .set_bg(back);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use ratatui::buffer::Cell;
    use std::ops::Range;

    const BLACK: Color = Color::Rgb(0, 0, 0);
    const WHITE: Color = Color::Rgb(255, 255, 255);
    const BASE: Color = Color::Rgb(25, 23, 36);
    const LOVE: Color = Color::Rgb(235, 111, 146);
    const FOAM: Color = Color::Rgb(156, 207, 216);

    /// A mix at either end is that end's colour exactly.
    ///
    /// Given: pairs of theme colours.
    /// When: each pair mixes at 0 and at 1, and past each end.
    /// Then: 0 and below give the first colour, and 1 and above give the
    /// second, to the last bit.
    ///
    /// Why it matters: a shade at its end must draw its own theme colour.
    /// A mix one step off draws a colour that no theme has.
    #[test]
    fn a_mix_at_either_end_is_that_colour() {
        let colours = [BLACK, WHITE, BASE, LOVE, FOAM];
        for from in colours {
            for to in colours {
                // When, then:
                assert_eq!(mix(from, to, 0.0), from, "{from:?} to {to:?} at 0");
                assert_eq!(mix(from, to, -0.5), from, "{from:?} to {to:?} below 0");
                assert_eq!(mix(from, to, 1.0), to, "{from:?} to {to:?} at 1");
                assert_eq!(mix(from, to, 1.5), to, "{from:?} to {to:?} above 1");
            }
        }
    }

    /// A mix between two colours is the point between them in OKLab.
    ///
    /// Given: black and white, love and foam, and the base and love.
    /// When: each pair mixes at a share between its ends.
    /// Then: each mix is the colour that coloraide 8.13, a colour library
    /// apart from this crate, gives for the same mix in OKLab, rounded to
    /// 8 bits.
    ///
    /// Why it matters: a mix in sRGB makes the middle of a fade too dark
    /// and too grey. OKLab keeps each step an even step to the eye.
    #[test]
    fn a_mix_is_the_point_between_in_oklab() {
        let cases = [
            (BLACK, WHITE, 0.5, Color::Rgb(99, 99, 99)),
            (LOVE, FOAM, 0.5, Color::Rgb(206, 163, 181)),
            (BASE, LOVE, 0.25, Color::Rgb(72, 44, 61)),
        ];
        for (from, to, share, expected) in cases {
            // When, then:
            assert_eq!(
                mix(from, to, share),
                expected,
                "{from:?} to {to:?} at {share}"
            );
        }
    }

    /// A colour that is no RGB value takes the nearer end of a mix.
    ///
    /// Given: the terminal's own background and an ink.
    /// When: they mix at shares below a half, at a half, and above it.
    /// Then: below a half the mix is the first colour, and from a half on
    /// it is the second.
    ///
    /// Why it matters: the terminal's own background has no value to mix
    /// with. A mix that guessed one would paint a colour the terminal does
    /// not show.
    #[test]
    fn a_colour_with_no_value_takes_the_nearer_end() {
        for (from, to) in [(Color::Reset, LOVE), (LOVE, Color::Reset)] {
            // When, then:
            assert_eq!(mix(from, to, 0.25), from, "{from:?} to {to:?}");
            assert_eq!(mix(from, to, 0.5), to, "{from:?} to {to:?}");
            assert_eq!(mix(from, to, 0.75), to, "{from:?} to {to:?}");
        }
    }

    const INK: Color = LOVE;
    const OTHER: Color = FOAM;
    const ROSE: Color = Color::Rgb(235, 188, 186);
    const BACK: Color = Color::Reset;

    /// One cell of `pixels`, in the order of the glyph's bits, as drawn.
    fn draw(pixels: [Color; 8]) -> Cell {
        let mut canvas = Canvas::new(1, 1);
        for (pixel, colour) in pixels.into_iter().enumerate() {
            canvas.set(pixel % CELL_WIDTH, pixel / CELL_WIDTH, colour);
        }
        let area = Rect::new(0, 0, 1, 1);
        let mut buffer = Buffer::empty(area);
        canvas.render(area, &mut buffer);
        buffer[(0, 0)].clone()
    }

    /// The colour each pixel of a drawn cell shows on screen.
    fn shown(cell: &Cell) -> [Color; 8] {
        let glyph = cell.symbol().chars().next().unwrap();
        let bits = OCTANTS
            .iter()
            .position(|&each| each == glyph)
            .unwrap_or_else(|| panic!("{glyph:?} is not an octant glyph"));
        std::array::from_fn(|pixel| {
            if bits >> pixel & 1 == 1 {
                cell.fg
            } else {
                cell.bg
            }
        })
    }

    /// Each row of `canvas` from `rows`, pixel by pixel.
    fn canvas_rows(canvas: &Canvas, rows: Range<usize>) -> Vec<Vec<Color>> {
        rows.map(|y| {
            (0..canvas.width())
                .map(|x| canvas.get(x, y).unwrap())
                .collect()
        })
        .collect()
    }

    /// A canvas of `columns` by `rows` cells with every pixel in `colour`.
    fn filled(columns: u16, rows: u16, colour: Color) -> Canvas {
        let mut canvas = Canvas::new(columns, rows);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                canvas.set(x, y, colour);
            }
        }
        canvas
    }

    /// The share of the way from `from` to `to` whose mix is `colour`, to
    /// the nearest 1/256, or none for a colour off the line between them.
    fn share_of(from: Color, to: Color, colour: Color) -> Option<f32> {
        (0..=256)
            .map(|step| step as f32 / 256.0)
            .find(|share| mix(from, to, *share) == colour)
    }

    /// Each pixel of a cell of two colours shows its own colour.
    ///
    /// Given: every pattern of eight pixels in the background and one ink,
    /// and every pattern in two inks.
    /// When: the cell is drawn.
    /// Then: each pixel shows its own colour.
    ///
    /// Why it matters: a cell that swaps its colours or its pixels draws
    /// every edge ragged.
    #[test]
    fn each_pixel_shows_its_own_colour() {
        for (on, off) in [(INK, BACK), (INK, OTHER)] {
            for bits in 0..=u8::MAX {
                // Given:
                let pixels: [Color; 8] =
                    std::array::from_fn(|pixel| if bits >> pixel & 1 == 1 { on } else { off });

                // When:
                let drawn = shown(&draw(pixels));

                // Then:
                assert_eq!(drawn, pixels, "{bits:08b} of {on:?} on {off:?}");
            }
        }
    }

    /// A cell of the background and one ink keeps the background behind.
    ///
    /// Given: every pattern of eight pixels in the background and one ink,
    /// with at least one pixel of the background.
    /// When: the cell is drawn.
    /// Then: its background is the terminal's own background.
    ///
    /// Why it matters: an ink behind a cell that is mostly ink would paint
    /// a block where the terminal should show.
    #[test]
    fn the_background_stays_behind_one_ink() {
        for bits in 0..u8::MAX {
            // Given:
            let pixels: [Color; 8] =
                std::array::from_fn(|pixel| if bits >> pixel & 1 == 1 { INK } else { BACK });

            // When:
            let cell = draw(pixels);

            // Then:
            assert_eq!(cell.bg, BACK, "{bits:08b}");
        }
    }

    /// A cell of more than two colours draws each pixel in the nearer
    /// colour of the pair that lies nearest all its pixels.
    ///
    /// Given: a cell that shades from the ink to foam, with three pixels
    /// of each and two of their mix at six tenths.
    /// When: the cell is drawn.
    /// Then: it shows the ink in its three ink pixels and foam in the rest.
    ///
    /// Why it matters: a gradient puts three or more colours in a cell.
    /// Each pixel in the nearer colour keeps the shade even across it.
    #[test]
    fn a_cell_of_more_colours_takes_the_pair_nearest_its_pixels() {
        // Given:
        let middle = mix(INK, OTHER, 0.6);
        let sea = [INK, INK, INK, middle, middle, OTHER, OTHER, OTHER];

        // When:
        let cell = draw(sea);

        // Then:
        assert_eq!(
            shown(&cell),
            [INK, INK, INK, OTHER, OTHER, OTHER, OTHER, OTHER]
        );
    }

    /// A cell of one ink draws a full block over the background.
    ///
    /// Given: a cell whose eight pixels are all the ink.
    /// When: the cell is drawn.
    /// Then: it shows a full block in the ink, with the terminal's own
    /// background behind it.
    ///
    /// Why it matters: a solid cell drawn as a space on an ink background
    /// paints the terminal's background over, which no other ink cell does.
    #[test]
    fn a_cell_of_one_ink_draws_a_full_block_over_the_background() {
        // When:
        let cell = draw([INK; 8]);

        // Then:
        assert_eq!((cell.symbol(), cell.fg, cell.bg), ("\u{2588}", INK, BACK));
    }

    /// A cell keeps the background behind when the background is one of
    /// its pair.
    ///
    /// Given: a cell of one pixel of the background, three of a colour near
    /// the ink, and four of the ink.
    /// When: the cell is drawn.
    /// Then: the background pixel shows the background, the other seven
    /// show the ink, and the background is behind.
    ///
    /// Why it matters: a pair of the two most common colours would give
    /// the background pixel an ink and hide the terminal there.
    #[test]
    fn a_cell_keeps_the_background_behind_in_its_pair() {
        // Given:
        let near = mix(INK, OTHER, 0.3);
        let pixels = [BACK, near, near, near, INK, INK, INK, INK];

        // When:
        let cell = draw(pixels);

        // Then:
        assert_eq!(shown(&cell), [BACK, INK, INK, INK, INK, INK, INK, INK]);
        assert_eq!(cell.bg, BACK);
    }

    /// Of two pairs that lie as near a cell's pixels, the pair whose
    /// colours come first in the cell wins.
    ///
    /// Given: four pixels of the background, two of the ink and two of
    /// rose, once with the ink first and once with rose first.
    /// When: each cell is drawn.
    /// Then: the colour that comes first fills all four of their pixels,
    /// with the background behind.
    ///
    /// Why it matters: a tie broken by chance draws the same instant in
    /// other colours each time.
    #[test]
    fn a_tie_between_two_pairs_goes_to_the_first_colours() {
        for (first, second) in [(INK, ROSE), (ROSE, INK)] {
            // Given:
            let pixels = [first, second, BACK, BACK, first, second, BACK, BACK];

            // When:
            let cell = draw(pixels);

            // Then:
            let expected = pixels.map(|colour| if colour == BACK { BACK } else { first });
            assert_eq!(shown(&cell), expected, "{first:?} first");
            assert_eq!(cell.bg, BACK, "{first:?} first");
        }
    }

    /// Two colours of equal count go in front by the order of their pixels.
    ///
    /// Given: a cell of two inks, four pixels each, the upper half of one
    /// and the lower half of the other, and the same cell swapped.
    /// When: each cell is drawn.
    /// Then: the ink of the upper half is the foreground of each.
    ///
    /// Why it matters: a tie broken by chance draws the same instant with
    /// other glyphs each time.
    #[test]
    fn a_tie_goes_to_the_first_pixel() {
        for (upper, lower) in [(INK, OTHER), (OTHER, INK)] {
            // Given:
            let pixels: [Color; 8] =
                std::array::from_fn(|pixel| if pixel < 4 { upper } else { lower });

            // When:
            let cell = draw(pixels);

            // Then:
            assert_eq!(
                (cell.symbol(), cell.fg, cell.bg),
                ("\u{2580}", upper, lower),
                "{upper:?} over {lower:?}"
            );
        }
    }

    /// A new canvas draws only the terminal's own background.
    ///
    /// Given: a canvas of three columns and two rows.
    /// When: it is drawn into a buffer of the same size.
    /// Then: each cell shows the background in all eight pixels.
    ///
    /// Why it matters: a canvas that started on an ink would paint a block
    /// over every cell that nothing draws on.
    #[test]
    fn a_new_canvas_draws_only_the_background() {
        // Given:
        let canvas = Canvas::new(3, 2);
        let area = Rect::new(0, 0, 3, 2);
        let mut buffer = Buffer::empty(area);

        // When:
        canvas.render(area, &mut buffer);

        // Then:
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(shown(&buffer[(x, y)]), [BACK; 8], "({x}, {y})");
            }
        }
    }

    /// A pixel set on the canvas draws, and one past an edge is dropped.
    ///
    /// Given: a canvas of one cell.
    /// When: one pixel inside it and one past each edge are set.
    /// Then: the cell shows the inside pixel in its colour, and the
    /// background in every other pixel.
    ///
    /// Why it matters: a pixel past the edge that wrapped to the next row
    /// would draw a stray dot at the far side.
    #[test]
    fn a_pixel_past_an_edge_is_dropped() {
        // Given:
        let mut canvas = Canvas::new(1, 1);
        let area = Rect::new(0, 0, 1, 1);
        let mut buffer = Buffer::empty(area);

        // When:
        canvas.set(1, 2, INK);
        canvas.set(2, 0, OTHER);
        canvas.set(0, 4, OTHER);
        canvas.render(area, &mut buffer);

        // Then:
        let mut expected = [BACK; 8];
        expected[2 * CELL_WIDTH + 1] = INK;
        assert_eq!(shown(&buffer[(0, 0)]), expected);
    }

    /// Each cell of the canvas lands at its own place in the area, and no
    /// cell lands past it.
    ///
    /// Given: a canvas of three columns and three rows, with an ink pixel
    /// in its first column and one in its last.
    /// When: it is drawn into an area of two columns and two rows at (4, 2)
    /// of a larger buffer.
    /// Then: the ink of the first column shows at its place in the area,
    /// and no cell outside the area changes.
    ///
    /// Why it matters: a canvas sized before the terminal shrank must not
    /// draw past its area into the next panel.
    #[test]
    fn each_cell_lands_in_its_place_in_the_area() {
        // Given:
        let mut canvas = Canvas::new(3, 3);
        canvas.set(1, 4, INK);
        canvas.set(5, 7, INK);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 6));

        // When:
        canvas.render(Rect::new(4, 2, 2, 2), &mut buffer);

        // Then:
        let mut ink = [BACK; 8];
        ink[1] = INK;
        for y in 0..6 {
            for x in 0..10 {
                let cell = &buffer[(x, y)];
                if !((4..6).contains(&x) && (2..4).contains(&y)) {
                    assert_eq!(cell, &Cell::default(), "({x}, {y}) is outside the area");
                } else if (x, y) == (4, 3) {
                    assert_eq!(shown(cell), ink, "({x}, {y})");
                } else {
                    assert_eq!(shown(cell), [BACK; 8], "({x}, {y})");
                }
            }
        }
    }

    /// A colour blends into a pixel by its coverage.
    ///
    /// Given: a canvas of foam with one pixel of ink.
    /// When: rose blends into pixels of foam at coverage 0, a quarter and
    /// 1, into the ink at a half, and into a pixel past an edge.
    /// Then: coverage 0 leaves foam, a quarter gives the mix of foam and
    /// rose at a quarter, 1 gives rose, and a half on the ink gives the mix
    /// of the ink and rose at a half. No other pixel changes.
    ///
    /// Why it matters: every soft edge is a blend. A blend that ignored the
    /// colour already there would draw a halo round a shape that lies over
    /// another.
    #[test]
    fn a_colour_blends_into_a_pixel_by_its_coverage() {
        // Given:
        let mut canvas = filled(2, 1, OTHER);
        canvas.set(3, 3, INK);

        // When:
        canvas.blend(0, 0, ROSE, 0.0);
        canvas.blend(1, 0, ROSE, 0.25);
        canvas.blend(2, 0, ROSE, 1.0);
        canvas.blend(3, 3, ROSE, 0.5);
        canvas.blend(4, 0, ROSE, 1.0);

        // Then:
        let mut expected = vec![vec![OTHER; 4]; 4];
        expected[0][1] = mix(OTHER, ROSE, 0.25);
        expected[0][2] = ROSE;
        expected[3][3] = mix(INK, ROSE, 0.5);
        assert_eq!(canvas_rows(&canvas, 0..4), expected);
    }

    /// A disc fills its inside, and blends its edge by the share of each
    /// pixel it covers.
    ///
    /// Given: a canvas of foam.
    /// When: a disc of ink with radius 2.5 is drawn round the centre of
    /// pixel (5, 6).
    /// Then: the pixels whose whole square lies inside show the ink, the
    /// pixels whose whole square lies outside show foam, the pixel at
    /// (7, 8) on the edge shows a mix strictly between them, and the shares
    /// of all pixels add up to the disc's area within half a pixel.
    ///
    /// Why it matters: a disc drawn whole pixel by pixel has a ragged rim,
    /// and a rim that covered too much would make a sun grow as it moves.
    #[test]
    fn a_disc_blends_its_edge_by_coverage() {
        // Given:
        let mut canvas = filled(6, 3, OTHER);
        let (centre, radius) = ((5.0, 6.0), 2.5);

        // When:
        canvas.disc(centre, radius, INK);

        // Then:
        let mut area = 0.0;
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let colour = canvas.get(x, y).unwrap();
                let share = share_of(OTHER, INK, colour)
                    .unwrap_or_else(|| panic!("({x}, {y}) is {colour:?}, no mix of foam and ink"));
                area += share;
                let (dx, dy) = (x as f32 - centre.0, y as f32 - centre.1);
                let far = (dx.abs() + 0.5).hypot(dy.abs() + 0.5);
                let near = (dx.abs() - 0.5).max(0.0).hypot((dy.abs() - 0.5).max(0.0));
                if far <= radius {
                    assert_eq!(colour, INK, "({x}, {y}) lies inside");
                } else if near >= radius {
                    assert_eq!(colour, OTHER, "({x}, {y}) lies outside");
                }
            }
        }
        let edge = canvas.get(7, 8).unwrap();
        assert_ne!(edge, INK, "the edge at (7, 8)");
        assert_ne!(edge, OTHER, "the edge at (7, 8)");
        let disc = std::f32::consts::PI * radius * radius;
        assert!(
            (area - disc).abs() < 0.5,
            "the shares add up to {area}, not {disc}"
        );
    }

    /// Over the terminal's own background an edge stays hard, and over
    /// another colour it blends.
    ///
    /// Given: a canvas with foam over its right half.
    /// When: a disc of ink is drawn across both halves, and the background
    /// blends into a pixel of foam at a quarter and at three quarters.
    /// Then: each pixel over the background is the background or the ink,
    /// never a mix. Over foam the disc's edge shows a mix of foam and the
    /// ink. The background at a quarter leaves the foam, and at three
    /// quarters replaces it.
    ///
    /// Why it matters: the app cannot know the colour of the terminal's
    /// background. A guessed mix would draw a dark or light ring round each
    /// shape.
    #[test]
    fn an_edge_over_the_background_stays_hard() {
        // Given:
        let mut canvas = Canvas::new(8, 4);
        let half = canvas.width() / 2;
        for y in 0..canvas.height() {
            for x in half..canvas.width() {
                canvas.set(x, y, OTHER);
            }
        }
        let foam = |x: usize| x >= half;

        // When:
        canvas.disc((8.0, 8.0), 5.3, INK);
        canvas.blend(12, 0, BACK, 0.25);
        canvas.blend(13, 0, BACK, 0.75);

        // Then:
        let mut soft = 0;
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let colour = canvas.get(x, y).unwrap();
                if (x, y) == (12, 0) {
                    assert_eq!(colour, OTHER, "the background at a quarter over foam");
                } else if (x, y) == (13, 0) {
                    assert_eq!(colour, BACK, "the background at three quarters over foam");
                } else if !foam(x) {
                    assert!(colour == BACK || colour == INK, "({x}, {y}) is {colour:?}");
                } else if colour != OTHER && colour != INK {
                    assert!(
                        share_of(OTHER, INK, colour).is_some(),
                        "({x}, {y}) is {colour:?}"
                    );
                    soft += 1;
                }
            }
        }
        assert!(soft > 0, "no pixel over foam blends");
    }
}
