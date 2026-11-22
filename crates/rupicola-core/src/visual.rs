//! Offline image and UI-tree measurements for visual verification. Nothing here launches an
//! application: it reads PNG files, frame sequences and a JSON tree that a toolkit-specific
//! helper produced.
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Largest per-channel difference still treated as unchanged, unless the caller
/// picks another.
pub const DEFAULT_DIFF_THRESHOLD: u8 = 8;
/// Channels per pixel in RGBA, RGB and grayscale-with-alpha images.
pub const RGBA_CHANNELS: usize = 4;
const RGB_CHANNELS: usize = 3;
const GRAY_ALPHA_CHANNELS: usize = 2;
/// Index of the alpha channel inside an RGBA pixel.
const ALPHA: usize = 3;
/// A byte channel at full intensity (and fully opaque alpha).
const CHANNEL_MAX: u8 = u8::MAX;
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

/// Corners of a changed region: x0, y0, x1, y1.
const BBOX_COORDS: usize = 4;

/// Diff image: changed pixels in solid red, unchanged ones as a pale gray
/// version of the new frame (darkened to a quarter, then lifted).
const DIFF_CHANGED: [u8; RGBA_CHANNELS] = [CHANNEL_MAX, 0, 0, CHANNEL_MAX];
const DIFF_DIM_DIVISOR: u8 = 4;
const DIFF_DIM_LIFT: u8 = 160;

/// Frame analysis: fewest frames that can show motion, the per-frame change
/// below which a frame counts as still, and the jump rule (big in absolute terms
/// and several times the median step).
const MIN_FRAMES: usize = 3;
const STILL_FRAME_RATIO: f64 = 0.0005;
const JUMP_MIN_RATIO: f64 = 0.05;
const JUMP_MEDIAN_FACTOR: f64 = 4.0;

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

/// Contact sheet: most images per sheet, gap around thumbnails, background.
const MAX_SHEET_IMAGES: usize = 64;
const SHEET_PADDING: u32 = 8;
const SHEET_BACKGROUND: [u8; RGBA_CHANNELS] = [90, 90, 90, CHANNEL_MAX];

/// Shell exit codes reported for capture commands that could not start or ran
/// too long, as `sh` and `timeout(1)` do.
const EXIT_NOT_RUNNABLE: i32 = 127;
const EXIT_TIMED_OUT: i32 = 124;
/// How often a running capture command is polled.
const COMMAND_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// UI audit thresholds (WCAG 2.2): smallest target, default font size, large
/// text sizes, required contrast ratios, the tolerance for comparing them and
/// for a rectangle poking outside the window, how many interactive nodes are
/// compared for overlap, the overlap share that counts, and the gap range that
/// looks like an accidental misalignment.
const MIN_TARGET_PX: f32 = 24.0;
const DEFAULT_FONT_PX: f32 = 14.0;
const LARGE_TEXT_PX: f32 = 24.0;
const LARGE_BOLD_TEXT_PX: f32 = 18.66;
const CONTRAST_LARGE: f64 = 3.0;
const CONTRAST_NORMAL: f64 = 4.5;
const CONTRAST_EPSILON: f64 = 1e-9;
const OFFSCREEN_TOLERANCE_PX: f32 = 0.5;
const MAX_OVERLAP_NODES: usize = 2000;
const OVERLAP_SHARE: f32 = 0.25;
const NEAR_ALIGNMENT_MIN_PX: f32 = 0.01;
const NEAR_ALIGNMENT_MAX_PX: f32 = 3.0;

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

// ---------- color ----------
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

// ---------- diff ----------
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

// ---------- frames ----------
#[derive(Debug, Serialize)]
pub struct Frames {
    pub frame_count: usize,
    pub step_changed_ratio: Vec<f64>,
    /// First and last frame compared: how much the whole animation changed.
    pub total_changed_ratio: f64,
    pub first_change: Option<usize>,
    pub settled_at: Option<usize>,
    /// Frames inside the active range that repeat the previous frame (dropped or late frames).
    pub stalls: Vec<usize>,
    /// Steps that change far more than the typical step (a jump instead of a tween).
    pub jumps: Vec<usize>,
    pub notes: Vec<String>,
}

fn natural_key(p: &Path) -> (String, u64) {
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix = name[..name.len() - digits.len()].to_owned();
    (prefix, digits.parse().unwrap_or(0))
}

/// # Errors
///
/// Returns an error if the directory cannot be opened.
pub fn list_pngs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("{}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .collect();
    files.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)).then(Ordering::Equal));
    Ok(files)
}

/// # Errors
///
/// Returns an error if fewer than three PNGs are found, a PNG cannot be read, or frame dimensions differ.
pub fn analyze_frames(dir: &Path, threshold: u8) -> Result<Frames> {
    let files = list_pngs(dir)?;
    if files.len() < MIN_FRAMES {
        bail!("need at least {MIN_FRAMES} PNG frames");
    }
    let images = files.iter().map(|p| load(p)).collect::<Result<Vec<_>>>()?;
    analyze_images(&images, threshold)
}

/// # Errors
///
/// Returns an error if there are fewer than three frames or their dimensions differ.
pub fn analyze_images(images: &[Image], threshold: u8) -> Result<Frames> {
    let [first, _, .., last] = images else {
        bail!("need at least {MIN_FRAMES} frames");
    };
    let mut ratios = Vec::new();
    for pair in images.windows(2) {
        let (d, _) = diff_images(&pair[0], &pair[1], threshold);
        if d.dimension_mismatch {
            bail!("frames have different dimensions");
        }
        ratios.push(d.changed_ratio);
    }
    let eps = STILL_FRAME_RATIO;
    let first_change = ratios.iter().position(|&r| r > eps).map(|i| i + 1);
    let settled_at = ratios.iter().rposition(|&r| r > eps).map(|i| i + 1);
    let total = diff_images(first, last, threshold).0.changed_ratio;
    let mut stalls = Vec::new();
    let mut jumps = Vec::new();
    let mut notes = Vec::new();
    if let (Some(f), Some(s)) = (first_change, settled_at) {
        let active: Vec<f64> = (f..=s).map(|frame| ratios[frame - 1]).collect();
        for frame in f..=s {
            if ratios[frame - 1] <= eps && frame != f && frame != s {
                stalls.push(frame);
            }
        }
        let mut nz: Vec<f64> = active.iter().copied().filter(|&r| r > eps).collect();
        nz.sort_by(f64::total_cmp);
        let median = nz.get(nz.len() / 2).copied().unwrap_or(0.0);
        for frame in f..=s {
            let r = ratios[frame - 1];
            if r > JUMP_MIN_RATIO && r > JUMP_MEDIAN_FACTOR * median {
                jumps.push(frame);
            }
        }
        if s - f + 1 < MIN_FRAMES {
            notes.push(format!(
                "the change spans fewer than {MIN_FRAMES} frames: it is effectively instant, not a tween"
            ));
        }
    } else {
        notes.push(
            "no frame changed: the animation did not run, or the capture missed it".to_owned(),
        );
    }
    if first_change.is_some() && total <= eps {
        notes.push("first and last frames match: the change returned to its start (check the capture window)".to_owned());
    }
    Ok(Frames {
        frame_count: images.len(),
        step_changed_ratio: ratios,
        total_changed_ratio: total,
        first_change,
        settled_at,
        stalls,
        jumps,
        notes,
    })
}

// ---------- palette ----------
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

// ---------- contact sheet ----------
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

// ---------- capture matrix ----------
#[derive(Debug, Serialize)]
pub struct MatrixEntry {
    pub name: String,
    pub env: Vec<(String, String)>,
    pub exit: Option<i32>,
    pub png: Option<PathBuf>,
    pub ok: bool,
}

pub struct MatrixSpec {
    pub states: Vec<String>,
    pub themes: Vec<String>,
    pub scales: Vec<String>,
    pub text_scales: Vec<String>,
    pub sizes: Vec<String>,
    pub locales: Vec<String>,
}

#[must_use]
pub fn matrix_plan(spec: &MatrixSpec, out: &Path) -> Vec<MatrixEntry> {
    let mut entries = Vec::new();
    for state in &spec.states {
        for theme in &spec.themes {
            for scale in &spec.scales {
                for text in &spec.text_scales {
                    for size in &spec.sizes {
                        for locale in &spec.locales {
                            let name =
                                format!("{state}__{theme}__x{scale}__t{text}__{size}__{locale}");
                            let (w, h) = size.split_once('x').unwrap_or((size.as_str(), "0"));
                            let png = out.join(format!("{name}.png"));
                            let env = vec![
                                ("RUPICOLA_OUT".to_owned(), png.display().to_string()),
                                ("RUPICOLA_STATE".to_owned(), state.clone()),
                                ("RUPICOLA_THEME".to_owned(), theme.clone()),
                                ("RUPICOLA_SCALE".to_owned(), scale.clone()),
                                ("RUPICOLA_TEXT_SCALE".to_owned(), text.clone()),
                                ("RUPICOLA_SIZE".to_owned(), size.clone()),
                                ("RUPICOLA_WIDTH".to_owned(), w.to_owned()),
                                ("RUPICOLA_HEIGHT".to_owned(), h.to_owned()),
                                ("RUPICOLA_LOCALE".to_owned(), locale.clone()),
                            ];
                            entries.push(MatrixEntry {
                                name,
                                env,
                                exit: None,
                                png: Some(png),
                                ok: false,
                            });
                        }
                    }
                }
            }
        }
    }
    entries
}

pub fn matrix_run(entries: &mut [MatrixEntry], cmd: &str, timeout: std::time::Duration) {
    for entry in entries.iter_mut() {
        let mut command = std::process::Command::new("sh");
        command
            .args(["-c", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        for (k, v) in &entry.env {
            command.env(k, v);
        }
        let Ok(mut child) = command.spawn() else {
            entry.exit = Some(EXIT_NOT_RUNNABLE);
            continue;
        };
        let start = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s.code(),
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Some(EXIT_TIMED_OUT);
                }
                Ok(None) => std::thread::sleep(COMMAND_POLL),
                Err(_) => break None,
            }
        };
        entry.exit = status;
        entry.ok = status == Some(0) && entry.png.as_ref().is_some_and(|p| p.is_file());
    }
}

// ---------- UI tree audit ----------
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub window: WindowSize,
    pub nodes: Vec<Node>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowSize {
    pub w: f32,
    pub h: f32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub font_px: Option<f32>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub clipped: bool,
    #[serde(default)]
    pub parent: Option<String>,
    /// "text" (default), "icon" or "boundary": non-text parts need 3:1.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditFinding {
    pub rule: &'static str,
    pub node: String,
    pub message: String,
}

pub const AUDIT_RULES: &[(&str, &str)] = &[
    (
        "target-size",
        "An interactive node is smaller than 24 x 24 px (WCAG 2.5.8, unless spacing applies).",
    ),
    (
        "missing-name",
        "An interactive node or an image has no accessible name.",
    ),
    (
        "contrast",
        "Text needs 4.5:1 (3:1 for large text), icons and boundaries 3:1.",
    ),
    ("clipped", "A node reports clipped content."),
    ("offscreen", "A node extends outside the window."),
    (
        "overlap",
        "Two interactive nodes overlap by more than a quarter of the smaller one.",
    ),
    (
        "near-alignment",
        "Siblings have left edges that differ by 1 to 3 px, which reads as a misalignment.",
    ),
];

/// Why a node's text or icon contrast falls short of WCAG 2.2, if it does.
fn contrast_shortfall(n: &Node) -> Option<String> {
    let fg: Rgb = n.fg.as_deref()?.parse().ok()?;
    let bg: Rgb = n.bg.as_deref()?.parse().ok()?;
    let non_text = matches!(n.kind.as_deref(), Some("icon" | "boundary"));
    let px = n.font_px.unwrap_or(DEFAULT_FONT_PX);
    let large = px >= LARGE_TEXT_PX || (n.bold && px >= LARGE_BOLD_TEXT_PX);
    let need = if non_text || large {
        CONTRAST_LARGE
    } else {
        CONTRAST_NORMAL
    };
    let got = fg.contrast(bg);
    (got + CONTRAST_EPSILON < need).then(|| format!("{got:.2}:1, needs {need}:1"))
}

#[must_use]
pub fn audit(tree: &Tree) -> Vec<AuditFinding> {
    let mut out = Vec::new();
    let mut add = |rule: &'static str, node: &str, message: String| {
        out.push(AuditFinding {
            rule,
            node: node.to_owned(),
            message,
        });
    };
    for n in &tree.nodes {
        if n.interactive && (n.w < MIN_TARGET_PX || n.h < MIN_TARGET_PX) {
            add(
                "target-size",
                &n.id,
                format!(
                    "{} x {} px is below {MIN_TARGET_PX} x {MIN_TARGET_PX} px",
                    n.w, n.h
                ),
            );
        }
        if (n.interactive || matches!(n.role.as_str(), "image" | "icon"))
            && n.name.as_deref().is_none_or(|s| s.trim().is_empty())
        {
            add(
                "missing-name",
                &n.id,
                format!("{} has no accessible name", n.role),
            );
        }
        if let Some(message) = contrast_shortfall(n) {
            add("contrast", &n.id, message);
        }
        if n.clipped {
            add("clipped", &n.id, "content is clipped".to_owned());
        }
        if n.x < -OFFSCREEN_TOLERANCE_PX
            || n.y < -OFFSCREEN_TOLERANCE_PX
            || n.x + n.w > tree.window.w + OFFSCREEN_TOLERANCE_PX
            || n.y + n.h > tree.window.h + OFFSCREEN_TOLERANCE_PX
        {
            add(
                "offscreen",
                &n.id,
                format!(
                    "rectangle {},{} {}x{} leaves the {}x{} window",
                    n.x, n.y, n.w, n.h, tree.window.w, tree.window.h
                ),
            );
        }
    }
    let interactive: Vec<&Node> = tree
        .nodes
        .iter()
        .filter(|n| n.interactive)
        .take(MAX_OVERLAP_NODES)
        .collect();
    for (i, a) in interactive.iter().enumerate() {
        for b in &interactive[i + 1..] {
            let ix = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
            let iy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
            if ix > 0.0 && iy > 0.0 {
                let smaller = (a.w * a.h).min(b.w * b.h).max(1.0);
                if ix * iy / smaller > OVERLAP_SHARE {
                    add("overlap", &a.id, format!("overlaps {}", b.id));
                }
            }
        }
    }
    let mut parents: std::collections::BTreeMap<&str, Vec<&Node>> =
        std::collections::BTreeMap::new();
    for n in &tree.nodes {
        if let Some(p) = n.parent.as_deref() {
            parents.entry(p).or_default().push(n);
        }
    }
    for kids in parents.values_mut() {
        kids.sort_by(|a, b| a.x.total_cmp(&b.x));
        for pair in kids.windows(2) {
            let d = pair[1].x - pair[0].x;
            if d > NEAR_ALIGNMENT_MIN_PX && d <= NEAR_ALIGNMENT_MAX_PX {
                add(
                    "near-alignment",
                    &pair[1].id,
                    format!("left edge is {d:.1} px from {}", pair[0].id),
                );
            }
        }
    }
    out
}
