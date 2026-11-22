//! Decoding and encoding PNG images.

use anyhow::{Context, Result, anyhow, bail};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// Channels per pixel in RGBA, RGB and grayscale-with-alpha images.
pub const RGBA_CHANNELS: usize = 4;
pub(super) const RGB_CHANNELS: usize = 3;
pub(super) const GRAY_ALPHA_CHANNELS: usize = 2;
/// Index of the alpha channel inside an RGBA pixel.
pub(super) const ALPHA: usize = 3;
/// A byte channel at full intensity (and fully opaque alpha).
pub(super) const CHANNEL_MAX: u8 = u8::MAX;

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// # Errors
///
/// Returns an error if the PNG cannot be read or decoded in a supported format.
pub fn load(path: &Path) -> Result<Image> {
    let file = File::open(path).with_context(|| format!("{}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("{}", path.display()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| anyhow!("image too large"))?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .with_context(|| format!("{}", path.display()))?;
    let data = &buf[..info.buffer_size()];
    let px = (info.width * info.height) as usize;
    let mut rgba = Vec::with_capacity(px * RGBA_CHANNELS);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => data
            .as_chunks::<RGB_CHANNELS>()
            .0
            .iter()
            .for_each(|c| rgba.extend_from_slice(&[c[0], c[1], c[2], CHANNEL_MAX])),
        png::ColorType::Grayscale => data
            .iter()
            .for_each(|&g| rgba.extend_from_slice(&[g, g, g, CHANNEL_MAX])),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<GRAY_ALPHA_CHANNELS>()
            .0
            .iter()
            .for_each(|c| rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]])),
        png::ColorType::Indexed => {
            bail!("{}: indexed PNG was not expanded", path.display());
        }
    }
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// # Errors
///
/// Returns an error if the output cannot be created or encoded as PNG.
pub fn save(path: &Path, image: &Image) -> Result<()> {
    let file = File::create(path).with_context(|| format!("{}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.rgba)?;
    Ok(())
}
