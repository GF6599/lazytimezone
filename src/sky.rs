//! An ambient sky for the empty band above the wall, on an octant
//! canvas: stars of three sizes and brightnesses by night, and clouds lit
//! on top and shaded underneath by day.
//!
//! Every pixel derives from its position and the frame's second, with no
//! stored state, so a frame drawn at the same instant is the same frame.
//! The sky moves only when the second changes.

use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};

/// The colours the sky draws in.
#[derive(Debug, Clone, Copy)]
pub struct SkyColours {
    pub dim: Color,
    pub middle: Color,
    /// A twinkling star at the top of its beat.
    pub bright: Color,
    pub lit: Color,
    pub shade: Color,
}

/// Out of every hundred cells: how many hold a star, and how many of
/// those twinkle.
const STARS_PER_HUNDRED: u64 = 5;
const TWINKLERS_PER_HUNDRED: u64 = 1;

const COLUMNS_PER_CLOUD: u16 = 24;
const SECONDS_PER_DRIFT: i64 = 4;

/// Paints the sky at `unix_seconds` over the whole of `canvas`.
pub fn paint(canvas: &mut Canvas, unix_seconds: i64, is_day: bool, colours: &SkyColours) {
    if is_day {
        clouds(canvas, unix_seconds, colours);
    } else {
        stars(canvas, unix_seconds, colours);
    }
}

/// A star in some cells, two pixels wide and one, two or three tall, so
/// the smallest is still two pixels. A twinkling star turns between bright
/// and dim each second.
fn stars(canvas: &mut Canvas, unix_seconds: i64, colours: &SkyColours) {
    let (columns, rows) = (canvas.width() / CELL_WIDTH, canvas.height() / CELL_HEIGHT);
    for y in 0..rows {
        for x in 0..columns {
            let roll = mix((x as u64) << 16 | y as u64);
            let (chance, pick) = (roll % 100, roll / 100);
            let colour = if chance < TWINKLERS_PER_HUNDRED {
                if (pick as i64 + unix_seconds).rem_euclid(2) == 0 {
                    colours.bright
                } else {
                    colours.dim
                }
            } else if chance < STARS_PER_HUNDRED {
                if pick / 7 % 2 == 0 {
                    colours.dim
                } else {
                    colours.middle
                }
            } else {
                continue;
            };
            let tall = 1 + (pick % 3) as usize;
            let top = y * CELL_HEIGHT + (pick / 3) as usize % (CELL_HEIGHT - tall + 1);
            for py in top..top + tall {
                for px in x * CELL_WIDTH..(x + 1) * CELL_WIDTH {
                    canvas.set(px, py, colour);
                }
            }
        }
    }
}

/// One cloud for every [`COLUMNS_PER_CLOUD`] columns, each a row of puffs
/// that rise toward its middle, all drifting one cell right every
/// [`SECONDS_PER_DRIFT`] seconds and wrapping at the edge. Each puff is
/// the shade, with a smaller lit puff a pixel higher over it, so the
/// light covers the top and the shade shows underneath.
fn clouds(canvas: &mut Canvas, unix_seconds: i64, colours: &SkyColours) {
    let (width, columns, rows) = (
        canvas.width(),
        (canvas.width() / CELL_WIDTH) as u16,
        (canvas.height() / CELL_HEIGHT) as u64,
    );
    if columns == 0 || rows == 0 {
        return;
    }
    let drift = unix_seconds.div_euclid(SECONDS_PER_DRIFT);
    for cloud in 0..(columns / COLUMNS_PER_CLOUD).max(1) {
        let roll = mix(u64::from(cloud) | 1 << 32);
        let length = 6 + (roll % 9) as usize;
        let row = ((roll / 16) % rows) as usize;
        let start = ((roll / 256 % u64::from(columns)) as i64 + drift)
            .rem_euclid(i64::from(columns)) as usize;
        let base = (row * CELL_HEIGHT + CELL_HEIGHT - 1) as f32;
        for wrap in [0, width] {
            for puff in 0..length {
                let rise = (std::f32::consts::PI * (puff as f32 + 0.5) / length as f32).sin();
                let radius = 1.6 + 1.2 * rise;
                let x = ((start + puff) * CELL_WIDTH) as f32 + 0.5 - wrap as f32;
                let y = base - radius + 0.5;
                canvas.disc((x, y), radius, colours.shade);
                canvas.disc((x, y - 0.8), radius - 0.4, colours.lit);
            }
        }
    }
}

/// splitmix64: spreads neighbouring inputs across the whole range, so
/// stars and clouds land with no visible pattern.
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
