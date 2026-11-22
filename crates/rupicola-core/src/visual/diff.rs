//! Pixel differences between two screenshots.

use super::{CHANNEL_MAX, Image, RGBA_CHANNELS};
use serde::Serialize;

/// Largest per-channel difference still treated as unchanged, unless the caller
/// picks another.
pub const DEFAULT_DIFF_THRESHOLD: u8 = 8;
/// Corners of a changed region: x0, y0, x1, y1.
const BBOX_COORDS: usize = 4;
/// Diff image: changed pixels in solid red, unchanged ones as a pale gray
/// version of the new frame (darkened to a quarter, then lifted).
const DIFF_CHANGED: [u8; RGBA_CHANNELS] = [CHANNEL_MAX, 0, 0, CHANNEL_MAX];
const DIFF_DIM_DIVISOR: u8 = 4;
const DIFF_DIM_LIFT: u8 = 160;

#[derive(Debug, Serialize)]
pub struct Diff {
    pub width: u32,
    pub height: u32,
    pub dimension_mismatch: bool,
    pub changed_pixels: u64,
    pub changed_ratio: f64,
    /// x0, y0, x1, y1 (inclusive) of the changed region.
    pub bbox: Option<[u32; BBOX_COORDS]>,
    pub mean_abs_delta: f64,
    pub max_delta: u8,
}

/// Pixels whose largest channel difference exceeds `threshold` count as changed.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the ratios and channel averages are reported as floating-point metrics"
)]
pub fn diff_images(before: &Image, after: &Image, threshold: u8) -> (Diff, Option<Image>) {
    if before.width != after.width || before.height != after.height {
        return (
            Diff {
                width: after.width,
                height: after.height,
                dimension_mismatch: true,
                changed_pixels: 0,
                changed_ratio: 1.0,
                bbox: None,
                mean_abs_delta: 0.0,
                max_delta: CHANNEL_MAX,
            },
            None,
        );
    }
    let (width, height) = (before.width as usize, before.height as usize);
    let mut out = Vec::with_capacity(width * height * RGBA_CHANNELS);
    let (mut changed, mut sum, mut max) = (0u64, 0u64, 0u8);
    let mut bbox: Option<[u32; BBOX_COORDS]> = None;
    for y in 0..before.height {
        for x in 0..before.width {
            let offset = (y as usize * width + x as usize) * RGBA_CHANNELS;
            let delta = (0..RGBA_CHANNELS)
                .map(|channel| before.rgba[offset + channel].abs_diff(after.rgba[offset + channel]))
                .max()
                .unwrap_or(0);
            sum += u64::from(delta);
            max = max.max(delta);
            if delta > threshold {
                changed += 1;
                bbox = Some(match bbox {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
                out.extend_from_slice(&DIFF_CHANGED);
            } else {
                let gray = after.rgba[offset] / DIFF_DIM_DIVISOR + DIFF_DIM_LIFT;
                out.extend_from_slice(&[gray, gray, gray, CHANNEL_MAX]);
            }
        }
    }
    let total = (width * height).max(1) as f64;
    (
        Diff {
            width: before.width,
            height: before.height,
            dimension_mismatch: false,
            changed_pixels: changed,
            changed_ratio: changed as f64 / total,
            bbox,
            mean_abs_delta: sum as f64 / total,
            max_delta: max,
        },
        Some(Image {
            width: before.width,
            height: before.height,
            rgba: out,
        }),
    )
}
