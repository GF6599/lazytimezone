//! Colour mixing in OKLab, for shades that change in even steps to the eye.

use std::sync::LazyLock;

use ratatui::style::Color;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
