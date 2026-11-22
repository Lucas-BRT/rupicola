//! Contact sheets that tile many images into one.

use super::{CHANNEL_MAX, Image, RGBA_CHANNELS, load};
use anyhow::{Result, anyhow, bail};
use std::path::PathBuf;

/// Contact sheet: most images per sheet, gap around thumbnails, background.
const MAX_SHEET_IMAGES: usize = 64;
const SHEET_PADDING: u32 = 8;
const SHEET_BACKGROUND: [u8; RGBA_CHANNELS] = [90, 90, 90, CHANNEL_MAX];

/// # Errors
///
/// Returns an error if the input count is outside 1–64, an image cannot be read, or the output dimensions or buffer exceed supported limits.
pub fn sheet(files: &[PathBuf], cols: usize, thumb_w: u32) -> Result<Image> {
    if files.is_empty() {
        bail!("no images");
    }
    if files.len() > MAX_SHEET_IMAGES {
        bail!("at most {MAX_SHEET_IMAGES} images per sheet");
    }
    let cols = cols.clamp(1, files.len());
    let thumbs = files
        .iter()
        .map(|p| {
            let im = load(p)?;
            let tw = thumb_w.min(im.width).max(1);
            let th = u32::try_from(
                (u64::from(im.height) * u64::from(tw) / u64::from(im.width.max(1))).max(1),
            )
            .map_err(|_| anyhow!("thumbnail height exceeds u32"))?;
            let mut px = Vec::new();
            for y in 0..th {
                let sy = usize::try_from(u64::from(y) * u64::from(im.height) / u64::from(th))
                    .map_err(|_| anyhow!("thumbnail row is out of range"))?;
                for x in 0..tw {
                    let sx = usize::try_from(u64::from(x) * u64::from(im.width) / u64::from(tw))
                        .map_err(|_| anyhow!("thumbnail column is out of range"))?;
                    let i = (sy * im.width as usize + sx) * RGBA_CHANNELS;
                    px.extend_from_slice(&im.rgba[i..i + RGBA_CHANNELS]);
                }
            }
            Ok(Image {
                width: tw,
                height: th,
                rgba: px,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let pad = SHEET_PADDING;
    let rows = thumbs.len().div_ceil(cols);
    let cell_w = thumbs.iter().map(|t| t.width).max().unwrap_or(1);
    let cell_h = thumbs.iter().map(|t| t.height).max().unwrap_or(1);
    let dimension = |count, cell: u32| {
        let count = u32::try_from(count).ok()?;
        cell.checked_add(pad)?.checked_mul(count)?.checked_add(pad)
    };
    let w = dimension(cols, cell_w).ok_or_else(|| anyhow!("sheet width exceeds u32"))?;
    let h = dimension(rows, cell_h).ok_or_else(|| anyhow!("sheet height exceeds u32"))?;
    let len = (w as usize)
        .checked_mul(h as usize)
        .and_then(|pixels| pixels.checked_mul(RGBA_CHANNELS))
        .ok_or_else(|| anyhow!("sheet buffer is too large"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(len)?;
    rgba.resize(len, 0);
    for px in rgba.as_chunks_mut::<RGBA_CHANNELS>().0 {
        px.copy_from_slice(&SHEET_BACKGROUND);
    }
    let pad = pad as usize;
    for (n, t) in thumbs.iter().enumerate() {
        let ox = pad + (n % cols) * (cell_w as usize + pad);
        let oy = pad + (n / cols) * (cell_h as usize + pad);
        let stride = t.width as usize * RGBA_CHANNELS;
        for (y, src) in t.rgba.chunks_exact(stride).enumerate() {
            let dst = ((oy + y) * w as usize + ox) * RGBA_CHANNELS;
            rgba[dst..dst + stride].copy_from_slice(src);
        }
    }
    Ok(Image {
        width: w,
        height: h,
        rgba,
    })
}
