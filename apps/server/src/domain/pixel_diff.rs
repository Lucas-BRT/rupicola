//! PNG pixel comparison and diff image generation for fix comparisons.

use anyhow::{Context, Result, bail};
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use std::io::Cursor;

/// Default per-channel tolerance (0..=255) before a pixel counts as changed.
pub const DEFAULT_CHANNEL_TOLERANCE: u8 = 4;
/// Highlight color (red) for changed pixels in the diff PNG.
const DIFF_HIGHLIGHT_RGBA: [u8; 4] = [239, 68, 68, 255];
/// Dimming divisor for unchanged pixels so diffs stand out.
const DIFF_DIM_DIVISOR: u8 = 3;
const BYTES_PER_PIXEL: usize = 4;

/// A rectangle in pixel coordinates, used for ignored regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(self, px: u32, py: u32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x.saturating_add(self.width)
            && py < self.y.saturating_add(self.height)
    }
}

/// Result of comparing two PNG screenshots.
#[derive(Clone, Debug)]
pub struct PixelDiff {
    pub changed_ratio: f64,
    pub diff_png: Vec<u8>,
}

struct DecodedRgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn decode_rgba(bytes: &[u8]) -> Result<DecodedRgba> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("read PNG header")?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).context("decode PNG frame")?;
    if info.bit_depth != BitDepth::Eight {
        bail!("unsupported PNG bit depth {:?}", info.bit_depth);
    }
    let raw = &buf[..info.buffer_size()];
    let pixel_count = usize::try_from(info.width)
        .ok()
        .and_then(|w| {
            usize::try_from(info.height)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .context("PNG dimensions overflow")?;
    let mut rgba = Vec::with_capacity(pixel_count * BYTES_PER_PIXEL);
    match info.color_type {
        ColorType::Rgba => rgba.extend_from_slice(raw),
        ColorType::Rgb => {
            for chunk in raw.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
        }
        ColorType::GrayscaleAlpha => {
            for chunk in raw.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
        }
        ColorType::Grayscale => {
            for &g in raw {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        ColorType::Indexed => bail!("indexed PNG was not expanded"),
    }
    Ok(DecodedRgba {
        width: info.width,
        height: info.height,
        pixels: rgba,
    })
}

fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = Encoder::new(&mut out, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().context("write diff PNG header")?;
        writer.write_image_data(rgba).context("write diff PNG")?;
    }
    Ok(out)
}

fn pixel_at(img: &DecodedRgba, x: u32, y: u32) -> Option<[u8; 4]> {
    if x >= img.width || y >= img.height {
        return None;
    }
    let idx = (y as usize * img.width as usize + x as usize) * BYTES_PER_PIXEL;
    Some([
        img.pixels[idx],
        img.pixels[idx + 1],
        img.pixels[idx + 2],
        img.pixels[idx + 3],
    ])
}

/// Computes the pixel difference ratio between `before` and `after` PNGs and renders a diff PNG.
///
/// # Errors
///
/// Fails when either input cannot be decoded as an 8-bit PNG or the diff PNG cannot be encoded.
pub fn diff_pngs(
    before: &[u8],
    after: &[u8],
    tolerance: u8,
    ignored: &[Rect],
) -> Result<PixelDiff> {
    let before = decode_rgba(before)?;
    let after = decode_rgba(after)?;
    let width = before.width.max(after.width);
    let height = before.height.max(after.height);
    let total = u64::from(width) * u64::from(height);
    if total == 0 {
        bail!("cannot diff empty PNG");
    }

    let mut diff_rgba = Vec::with_capacity(usize::try_from(total).unwrap_or(0) * BYTES_PER_PIXEL);
    let mut changed: u64 = 0;

    for y in 0..height {
        for x in 0..width {
            let is_ignored = ignored.iter().any(|rect| rect.contains(x, y));
            let px_before = pixel_at(&before, x, y);
            let px_after = pixel_at(&after, x, y);
            let differs = !is_ignored
                && match (px_before, px_after) {
                    (Some(a), Some(b)) => (0..4).any(|i| a[i].abs_diff(b[i]) > tolerance),
                    _ => true,
                };
            if differs {
                changed += 1;
                diff_rgba.extend_from_slice(&DIFF_HIGHLIGHT_RGBA);
            } else {
                let base = px_after.or(px_before).unwrap_or([0, 0, 0, 255]);
                diff_rgba.extend_from_slice(&[
                    base[0] / DIFF_DIM_DIVISOR,
                    base[1] / DIFF_DIM_DIVISOR,
                    base[2] / DIFF_DIM_DIVISOR,
                    base[3],
                ]);
            }
        }
    }

    let diff_png = encode_rgba(width, height, &diff_rgba)?;
    #[allow(clippy::cast_precision_loss)]
    let changed_ratio = changed as f64 / total as f64;
    Ok(PixelDiff {
        changed_ratio,
        diff_png,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&color);
        }
        encode_rgba(width, height, &rgba).unwrap()
    }

    #[test]
    fn identical_pngs_have_zero_changed_ratio() {
        let img = make_png(4, 4, [10, 20, 30, 255]);
        let diff = diff_pngs(&img, &img, DEFAULT_CHANNEL_TOLERANCE, &[]).unwrap();
        assert!((diff.changed_ratio - 0.0).abs() < f64::EPSILON);
        assert!(decode_rgba(&diff.diff_png).is_ok());
    }

    #[test]
    fn channel_tolerance_and_ignored_regions_are_respected() {
        let a = make_png(4, 4, [100, 100, 100, 255]);
        let within = make_png(4, 4, [104, 100, 100, 255]);
        let over = make_png(4, 4, [105, 100, 100, 255]);

        assert!(
            diff_pngs(&a, &within, DEFAULT_CHANNEL_TOLERANCE, &[])
                .unwrap()
                .changed_ratio
                .abs()
                < f64::EPSILON
        );
        assert!(
            (diff_pngs(&a, &over, DEFAULT_CHANNEL_TOLERANCE, &[])
                .unwrap()
                .changed_ratio
                - 1.0)
                .abs()
                < f64::EPSILON
        );
        let ignored = [Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        }];
        assert!(
            diff_pngs(&a, &over, DEFAULT_CHANNEL_TOLERANCE, &ignored)
                .unwrap()
                .changed_ratio
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn mismatched_dimensions_count_extra_pixels_as_changed() {
        let small = make_png(2, 2, [10, 10, 10, 255]);
        let tall = make_png(2, 4, [10, 10, 10, 255]);
        let diff = diff_pngs(&small, &tall, DEFAULT_CHANNEL_TOLERANCE, &[]).unwrap();
        assert!((diff.changed_ratio - 0.5).abs() < f64::EPSILON);
    }
}
