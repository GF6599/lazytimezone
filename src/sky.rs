//! An ambient sky for the empty band above the wall: Braille stars by
//! night, drifting Braille clouds by day.
//!
//! Every cell derives from its position and the frame's second, with no
//! stored state, so a frame drawn at the same instant is the same frame.
//! The sky moves only on the app's one-second tick.

/// One lit cell of the sky. A bright cell is a twinkling star at the top
/// of its beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyCell {
    pub glyph: char,
    pub bright: bool,
}

const STAR_GLYPHS: [char; 8] = [
    '\u{2801}', '\u{2802}', '\u{2804}', '\u{2840}', '\u{2808}', '\u{2810}', '\u{2820}', '\u{2880}',
];
const TWINKLE_GLYPHS: [char; 2] = ['\u{283f}', '\u{2802}'];
/// Out of every hundred cells: how many hold a star, and how many of
/// those twinkle.
const STARS_PER_HUNDRED: u64 = 5;
const TWINKLERS_PER_HUNDRED: u64 = 1;

const CLOUD_EDGE: char = '\u{28c0}';
const CLOUD_BODY: char = '\u{28e4}';
const COLUMNS_PER_CLOUD: u16 = 24;
const SECONDS_PER_DRIFT: i64 = 4;

/// Lays out a `width` by `height` sky at `unix_seconds`, row by row.
pub fn render(
    width: u16,
    height: u16,
    unix_seconds: i64,
    is_day: bool,
) -> Vec<Vec<Option<SkyCell>>> {
    if is_day {
        clouds(width, height, unix_seconds)
    } else {
        stars(width, height, unix_seconds)
    }
}

fn stars(width: u16, height: u16, unix_seconds: i64) -> Vec<Vec<Option<SkyCell>>> {
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| {
                    let roll = mix(u64::from(x) << 16 | u64::from(y));
                    let (chance, pick) = (roll % 100, roll / 100);
                    if chance < TWINKLERS_PER_HUNDRED {
                        let beat = (pick as i64 + unix_seconds).rem_euclid(2) as usize;
                        Some(SkyCell {
                            glyph: TWINKLE_GLYPHS[beat],
                            bright: beat == 0,
                        })
                    } else if chance < STARS_PER_HUNDRED {
                        Some(SkyCell {
                            glyph: STAR_GLYPHS[(pick % 8) as usize],
                            bright: false,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .collect()
}

/// One cloud for every [`COLUMNS_PER_CLOUD`] columns, each a long mound
/// with a shorter one under it, all drifting one cell right every
/// [`SECONDS_PER_DRIFT`] seconds and wrapping at the edge.
fn clouds(width: u16, height: u16, unix_seconds: i64) -> Vec<Vec<Option<SkyCell>>> {
    let mut sky = vec![vec![None; usize::from(width)]; usize::from(height)];
    if width == 0 || height == 0 {
        return sky;
    }
    let drift = unix_seconds.div_euclid(SECONDS_PER_DRIFT);
    for cloud in 0..(width / COLUMNS_PER_CLOUD).max(1) {
        let roll = mix(u64::from(cloud) | 1 << 32);
        let length = 6 + (roll % 9) as i64;
        let row = ((roll / 16) % u64::from(height)) as usize;
        let start = (roll / 256 % u64::from(width)) as i64 + drift;
        lay_mound(&mut sky[row], start, length);
        if let Some(lower) = sky.get_mut(row + 1) {
            lay_mound(lower, start + 2, length - 4);
        }
    }
    sky
}

/// Writes `⣀⣤…⣤⣀` from `start`, wrapping at the row's end.
fn lay_mound(row: &mut [Option<SkyCell>], start: i64, length: i64) {
    let width = row.len() as i64;
    for i in 0..length {
        let glyph = if i == 0 || i == length - 1 {
            CLOUD_EDGE
        } else {
            CLOUD_BODY
        };
        row[(start + i).rem_euclid(width) as usize] = Some(SkyCell {
            glyph,
            bright: false,
        });
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
