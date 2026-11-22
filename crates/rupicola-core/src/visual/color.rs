//! sRGB colors: WCAG luminance and contrast, hex parsing and printing.

use super::RGB_CHANNELS;
use anyhow::Result;

const CHANNEL_MAX_F: f64 = 255.0;
/// sRGB to linear light (WCAG 2 relative luminance).
const SRGB_LINEAR_THRESHOLD: f64 = 0.04045;
const SRGB_LINEAR_SLOPE: f64 = 12.92;
const SRGB_OFFSET: f64 = 0.055;
const SRGB_GAMMA: f64 = 2.4;
/// Luminance weight of each linear channel.
const LUMA_RED: f64 = 0.2126;
const LUMA_GREEN: f64 = 0.7152;
const LUMA_BLUE: f64 = 0.0722;
/// Flare term added to both luminances in the WCAG contrast ratio.
const CONTRAST_FLARE: f64 = 0.05;
/// Hex digits in `#rrggbb` and `#rgb`, and the factor that widens one digit to a byte.
const HEX_LONG_LEN: usize = 6;
const HEX_SHORT_LEN: usize = 3;
const HEX_DIGIT_TO_BYTE: u8 = 0x11;
const HEX_RADIX: u32 = 16;

/// An opaque sRGB color, one byte per channel. Parse it from `#rrggbb` or `#rgb`
/// with [`str::parse`] and write it back as `#rrggbb` with [`std::fmt::Display`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// A neutral gray with every channel at `level`.
    #[must_use]
    pub const fn gray(level: u8) -> Self {
        Self::new(level, level, level)
    }

    /// WCAG 2 relative luminance, from 0 (black) to 1 (white).
    #[must_use]
    pub fn luminance(self) -> f64 {
        LUMA_RED * linear(self.r) + LUMA_GREEN * linear(self.g) + LUMA_BLUE * linear(self.b)
    }

    /// WCAG 2 contrast ratio against `other`, from 1:1 to 21:1.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + CONTRAST_FLARE) / (lo + CONTRAST_FLARE)
    }

    /// HSV saturation: 0 for grays, 1 for fully saturated colors.
    #[must_use]
    pub fn saturation(self) -> f64 {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        if max == 0 {
            0.0
        } else {
            f64::from(max - min) / f64::from(max)
        }
    }
}

/// sRGB channel to linear light.
fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / CHANNEL_MAX_F;
    if c <= SRGB_LINEAR_THRESHOLD {
        c / SRGB_LINEAR_SLOPE
    } else {
        ((c + SRGB_OFFSET) / (1.0 + SRGB_OFFSET)).powf(SRGB_GAMMA)
    }
}

impl From<[u8; RGB_CHANNELS]> for Rgb {
    fn from([r, g, b]: [u8; RGB_CHANNELS]) -> Self {
        Self::new(r, g, b)
    }
}

impl From<Rgb> for [u8; RGB_CHANNELS] {
    fn from(color: Rgb) -> Self {
        [color.r, color.g, color.b]
    }
}

/// Text that is not a `#rrggbb` or `#rgb` color.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a #rrggbb or #rgb color")]
pub struct InvalidColor;

impl std::str::FromStr for Rgb {
    type Err = InvalidColor;

    fn from_str(text: &str) -> Result<Self, InvalidColor> {
        let t = text.trim().trim_start_matches('#');
        let byte = |digits: &str| u8::from_str_radix(digits, HEX_RADIX).map_err(|_| InvalidColor);
        match t.len() {
            HEX_LONG_LEN => Ok(Self::new(byte(&t[0..2])?, byte(&t[2..4])?, byte(&t[4..6])?)),
            HEX_SHORT_LEN => Ok(Self::new(
                byte(&t[0..1])? * HEX_DIGIT_TO_BYTE,
                byte(&t[1..2])? * HEX_DIGIT_TO_BYTE,
                byte(&t[2..3])? * HEX_DIGIT_TO_BYTE,
            )),
            _ => Err(InvalidColor),
        }
    }
}

impl std::fmt::Display for Rgb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}
