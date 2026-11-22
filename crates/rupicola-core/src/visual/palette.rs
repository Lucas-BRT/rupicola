//! Dominant colors and suggested design roles.

use super::{ALPHA, Image, RGB_CHANNELS, RGBA_CHANNELS, Rgb};
use serde::Serialize;

/// Palette extraction: alpha below which a pixel is ignored, bits kept per
/// channel for the histogram, RGB distance that merges two buckets, and the
/// shares and thresholds that pick each role.
const OPAQUE_ALPHA: u8 = 128;
const HISTOGRAM_BITS: u8 = 4;
const CLUSTER_DISTANCE: f64 = 48.0;
const TEXT_MIN_SHARE: f64 = 0.005;
const ACCENT_MIN_SHARE: f64 = 0.002;
const ACCENT_MIN_SATURATION: f64 = 0.25;
const SURFACE_MAX_CONTRAST: f64 = 1.5;
const SURFACE_MIN_SHARE: f64 = 0.02;
/// Decimal places kept when reporting contrast and color shares.
const CONTRAST_ROUNDING: f64 = 100.0;
const SHARE_ROUNDING: f64 = 1000.0;

#[derive(Debug, Serialize)]
pub struct Swatch {
    pub hex: String,
    pub share: f64,
}
#[derive(Debug, Serialize)]
pub struct Palette {
    pub colors: Vec<Swatch>,
    pub suggested: Roles,
    pub note: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Roles {
    pub background: Option<String>,
    pub text: Option<String>,
    pub text_contrast: Option<f64>,
    pub accent: Option<String>,
    pub surface: Option<String>,
}

/// Bits in one color channel.
const CHANNEL_BITS: u8 = 8;

/// Histogram bucket of a pixel: the top [`HISTOGRAM_BITS`] of each RGB channel.
fn histogram_key(px: [u8; RGBA_CHANNELS]) -> u16 {
    let shift = CHANNEL_BITS - HISTOGRAM_BITS;
    (u16::from(px[0] >> shift) << (2 * HISTOGRAM_BITS))
        | (u16::from(px[1] >> shift) << HISTOGRAM_BITS)
        | u16::from(px[2] >> shift)
}

#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "color clustering and population shares are approximate floating-point calculations"
)]
pub fn palette(image: &Image, k: usize) -> Palette {
    // Histogram over 4 bits per channel, then greedy merge of nearby buckets.
    let mut buckets: std::collections::HashMap<u16, (u64, [u64; RGB_CHANNELS])> =
        std::collections::HashMap::new();
    let mut total = 0u64;
    for px in image.rgba.as_chunks::<RGBA_CHANNELS>().0 {
        if px[ALPHA] < OPAQUE_ALPHA {
            continue;
        }
        total += 1;
        let e = buckets
            .entry(histogram_key(*px))
            .or_insert((0, [0; RGB_CHANNELS]));
        e.0 += 1;
        for (sum, v) in e.1.iter_mut().zip(px) {
            *sum += u64::from(*v);
        }
    }
    let mut sorted: Vec<(u64, [f64; RGB_CHANNELS])> = buckets
        .into_values()
        .map(|(n, s)| {
            (
                n,
                [
                    s[0] as f64 / n as f64,
                    s[1] as f64 / n as f64,
                    s[2] as f64 / n as f64,
                ],
            )
        })
        .collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut clusters: Vec<(u64, [f64; RGB_CHANNELS])> = Vec::new();
    for (n, c) in sorted {
        let near = clusters.iter_mut().find(|(_, cc)| {
            ((cc[0] - c[0]).powi(2) + (cc[1] - c[1]).powi(2) + (cc[2] - c[2]).powi(2)).sqrt()
                < CLUSTER_DISTANCE
        });
        match near {
            Some((cn, cc)) => {
                let t = (*cn + n) as f64;
                for i in 0..RGB_CHANNELS {
                    cc[i] = (cc[i] * *cn as f64 + c[i] * n as f64) / t;
                }
                *cn += n;
            }
            None => clusters.push((n, c)),
        }
    }
    clusters.sort_by_key(|b| std::cmp::Reverse(b.0));
    clusters.truncate(k.max(1));
    let colors: Vec<(Rgb, f64)> = clusters
        .iter()
        .map(|(n, c)| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "RGB centroids are weighted averages of byte values and rounded to bytes"
            )]
            let rgb = Rgb::from(c.map(|channel| channel.round() as u8));
            (rgb, *n as f64 / total.max(1) as f64)
        })
        .collect();
    let background = colors.first().map(|c| c.0);
    let text = background.and_then(|bg| {
        colors
            .iter()
            .filter(|c| c.1 >= TEXT_MIN_SHARE)
            .map(|c| (c.0, c.0.contrast(bg)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    });
    let accent = colors
        .iter()
        .skip(1)
        .filter(|c| c.1 >= ACCENT_MIN_SHARE && Some(c.0) != text.map(|t| t.0))
        .max_by(|a, b| a.0.saturation().total_cmp(&b.0.saturation()))
        .filter(|c| c.0.saturation() > ACCENT_MIN_SATURATION)
        .map(|c| c.0);
    let surface = background.and_then(|bg| {
        colors
            .iter()
            .skip(1)
            .find(|c| c.0.contrast(bg) < SURFACE_MAX_CONTRAST && c.1 >= SURFACE_MIN_SHARE)
            .map(|c| c.0)
    });
    Palette {
        suggested: Roles {
            background: background.map(|color| color.to_string()),
            text: text.map(|t| t.0.to_string()),
            text_contrast: text.map(|t| (t.1 * CONTRAST_ROUNDING).round() / CONTRAST_ROUNDING),
            accent: accent.map(|color| color.to_string()),
            surface: surface.map(|color| color.to_string()),
        },
        colors: colors
            .into_iter()
            .map(|(c, s)| Swatch {
                hex: c.to_string(),
                share: (s * SHARE_ROUNDING).round() / SHARE_ROUNDING,
            })
            .collect(),
        note: "Heuristic: the most common color is taken as the background, so verify roles against the reference by eye.",
    }
}
