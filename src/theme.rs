//! # Theme system — seven colour palettes.
//!
//! Each theme is a flat [`ThemeColors`] struct with semantic colour
//! slots (`accent`, `good`, `warning`, …) rather than component-
//! specific styles. The UI layer maps these slots to ratatui
//! [`Style`](ratatui::style::Style) values, keeping theme definitions
//! decoupled from widget details.
//!
//! Persistence is handled by the [`config`](crate::config) module.

use ratatui::style::Color;

/// Available colour themes, cycled with the `t` key.
///
/// Variants are ordered to match the cycle sequence in [`Theme::next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Default,
    Dracula,
    Solarized,
    Nord,
    Monokai,
    Gruvbox,
    RosePine,
}

impl Theme {
    pub fn label(&self) -> &'static str {
        match self {
            Theme::Default => "Default",
            Theme::Dracula => "Dracula",
            Theme::Solarized => "Solarized",
            Theme::Nord => "Nord",
            Theme::Monokai => "Monokai",
            Theme::Gruvbox => "Gruvbox",
            Theme::RosePine => "Rose Pine",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Theme::Default => Theme::Dracula,
            Theme::Dracula => Theme::Solarized,
            Theme::Solarized => Theme::Nord,
            Theme::Nord => Theme::Monokai,
            Theme::Monokai => Theme::Gruvbox,
            Theme::Gruvbox => Theme::RosePine,
            Theme::RosePine => Theme::Default,
        }
    }

    pub fn colors(&self) -> ThemeColors {
        match self {
            Theme::Default => default_colors(),
            Theme::Dracula => dracula_colors(),
            Theme::Solarized => solarized_colors(),
            Theme::Nord => nord_colors(),
            Theme::Monokai => monokai_colors(),
            Theme::Gruvbox => gruvbox_colors(),
            Theme::RosePine => rose_pine_colors(),
        }
    }

    pub fn from_label(s: &str) -> Self {
        Self::try_from_label(s).unwrap_or(Theme::Default)
    }

    /// Strict, case-insensitive label lookup.
    ///
    /// Returns `None` for unknown labels so callers (e.g. the config-load
    /// path) can surface a warning instead of silently downgrading to
    /// [`Theme::Default`].
    pub fn try_from_label(label: &str) -> Option<Theme> {
        if label.eq_ignore_ascii_case("Default") {
            Some(Theme::Default)
        } else if label.eq_ignore_ascii_case("Dracula") {
            Some(Theme::Dracula)
        } else if label.eq_ignore_ascii_case("Solarized") {
            Some(Theme::Solarized)
        } else if label.eq_ignore_ascii_case("Nord") {
            Some(Theme::Nord)
        } else if label.eq_ignore_ascii_case("Monokai") {
            Some(Theme::Monokai)
        } else if label.eq_ignore_ascii_case("Gruvbox") {
            Some(Theme::Gruvbox)
        } else if label.eq_ignore_ascii_case("Rose Pine") {
            Some(Theme::RosePine)
        } else {
            None
        }
    }
}

// ============================================================================
// Colour palette
// ============================================================================

/// Semantic colour slots consumed by the UI layer.
///
/// All themes use hardcoded RGB values for consistent rendering across
/// terminals. Only `bg` uses `Color::Reset` for terminal background
/// transparency.
pub struct ThemeColors {
    pub bg: Color,
    pub fg: Color,
    /// De-emphasised text (countries, regions, date lines).
    pub muted: Color,
    pub border: Color,
    pub title: Color,
    /// Primary accent — daytime clock digits, search-active border.
    pub accent: Color,
    /// Secondary accent — nighttime clock digits.
    pub accent_secondary: Color,
    /// Positive semantic colour (daytime local times, "Copied!" flash).
    pub good: Color,
    /// Informational colour (diff column, UTC offset).
    pub info: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    /// Colour for the favourite star glyph (★).
    pub star: Color,
}

fn default_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(200, 204, 212),
        muted: Color::Rgb(108, 115, 128),
        border: Color::Rgb(68, 74, 87),
        title: Color::Rgb(126, 207, 154),
        accent: Color::Rgb(100, 200, 220),
        accent_secondary: Color::Rgb(228, 196, 108),
        good: Color::Rgb(126, 207, 154),
        info: Color::Rgb(100, 200, 220),
        status_bg: Color::Rgb(90, 130, 180),
        status_fg: Color::Rgb(235, 238, 245),
        star: Color::Rgb(228, 196, 108),
    }
}

fn dracula_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(248, 248, 242),
        muted: Color::Rgb(98, 114, 164),
        border: Color::Rgb(68, 71, 90),
        title: Color::Rgb(80, 250, 123),
        accent: Color::Rgb(139, 233, 253),
        accent_secondary: Color::Rgb(241, 250, 140),
        good: Color::Rgb(80, 250, 123),
        info: Color::Rgb(139, 233, 253),
        status_bg: Color::Rgb(189, 147, 249),
        status_fg: Color::Rgb(40, 42, 54),
        star: Color::Rgb(241, 250, 140),
    }
}

fn solarized_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(131, 148, 150),
        muted: Color::Rgb(88, 110, 117),
        border: Color::Rgb(7, 54, 66),
        title: Color::Rgb(133, 153, 0),
        accent: Color::Rgb(38, 139, 210),
        accent_secondary: Color::Rgb(181, 137, 0),
        good: Color::Rgb(133, 153, 0),
        info: Color::Rgb(38, 139, 210),
        status_bg: Color::Rgb(42, 161, 152),
        status_fg: Color::Rgb(0, 43, 54),
        star: Color::Rgb(181, 137, 0),
    }
}

fn nord_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(216, 222, 233),
        muted: Color::Rgb(76, 86, 106),
        border: Color::Rgb(59, 66, 82),
        title: Color::Rgb(163, 190, 140),
        accent: Color::Rgb(136, 192, 208),
        accent_secondary: Color::Rgb(235, 203, 139),
        good: Color::Rgb(163, 190, 140),
        info: Color::Rgb(136, 192, 208),
        status_bg: Color::Rgb(129, 161, 193),
        status_fg: Color::Rgb(46, 52, 64),
        star: Color::Rgb(235, 203, 139),
    }
}

fn monokai_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(248, 248, 242),
        muted: Color::Rgb(117, 113, 94),
        border: Color::Rgb(73, 72, 62),
        title: Color::Rgb(166, 226, 46),
        accent: Color::Rgb(102, 217, 239),
        accent_secondary: Color::Rgb(253, 151, 31),
        good: Color::Rgb(166, 226, 46),
        info: Color::Rgb(102, 217, 239),
        status_bg: Color::Rgb(174, 129, 255),
        status_fg: Color::Rgb(39, 40, 34),
        star: Color::Rgb(253, 151, 31),
    }
}

fn gruvbox_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(235, 219, 178),
        muted: Color::Rgb(146, 131, 116),
        border: Color::Rgb(80, 73, 69),
        title: Color::Rgb(184, 187, 38),
        accent: Color::Rgb(131, 165, 152),
        accent_secondary: Color::Rgb(250, 189, 47),
        good: Color::Rgb(184, 187, 38),
        info: Color::Rgb(131, 165, 152),
        status_bg: Color::Rgb(211, 134, 155),
        status_fg: Color::Rgb(40, 40, 40),
        star: Color::Rgb(250, 189, 47),
    }
}

/// The main Rose Pine palette, with each slot's role named beside it.
/// Leaf, the only green, comes from rose-pine/neovim, because the
/// palette itself has none for the `good` slot.
fn rose_pine_colors() -> ThemeColors {
    ThemeColors {
        bg: Color::Reset,
        fg: Color::Rgb(224, 222, 244),               // text
        muted: Color::Rgb(110, 106, 134),            // muted
        border: Color::Rgb(64, 61, 82),              // highlight med
        title: Color::Rgb(235, 188, 186),            // rose
        accent: Color::Rgb(156, 207, 216),           // foam
        accent_secondary: Color::Rgb(196, 167, 231), // iris
        good: Color::Rgb(149, 177, 172),             // leaf
        info: Color::Rgb(156, 207, 216),             // foam
        status_bg: Color::Rgb(49, 116, 143),         // pine
        status_fg: Color::Rgb(224, 222, 244),        // text
        star: Color::Rgb(246, 193, 119),             // gold
    }
}
