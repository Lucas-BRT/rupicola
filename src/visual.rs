//! Offline image and UI-tree measurements for visual verification. Nothing here launches an
//! application: it reads PNG files, frame sequences and a JSON tree that a toolkit-specific
//! helper produced.
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn err<T: ToString>(x: T) -> String {
    x.to_string()
}

/// # Errors
///
/// Returns an error if the PNG cannot be read or decoded in a supported format.
pub fn load(path: &Path) -> Result<Image, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let size = reader.output_buffer_size().ok_or("image too large")?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let data = &buf[..info.buffer_size()];
    let px = (info.width * info.height) as usize;
    let mut rgba = Vec::with_capacity(px * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|c| rgba.extend_from_slice(&[c[0], c[1], c[2], 255])),
        png::ColorType::Grayscale => data
            .iter()
            .for_each(|&g| rgba.extend_from_slice(&[g, g, g, 255])),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .for_each(|c| rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]])),
        png::ColorType::Indexed => {
            return Err(format!("{}: indexed PNG was not expanded", path.display()));
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
pub fn save(path: &Path, image: &Image) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(err)?;
    writer.write_image_data(&image.rgba).map_err(err)
}

// ---------- contrast ----------
fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
#[must_use]
pub fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])
}
#[must_use]
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}
#[must_use]
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let t = text.trim().trim_start_matches('#');
    let v = |s: &str| u8::from_str_radix(s, 16).ok();
    match t.len() {
        6 => Some([v(&t[0..2])?, v(&t[2..4])?, v(&t[4..6])?]),
        3 => Some([v(&t[0..1])? * 17, v(&t[1..2])? * 17, v(&t[2..3])? * 17]),
        _ => None,
    }
}
fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
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
    pub bbox: Option<[u32; 4]>,
    pub mean_abs_delta: f64,
    pub max_delta: u8,
}

/// Pixels whose largest channel difference exceeds `threshold` count as changed.
#[must_use]
pub fn diff_images(a: &Image, b: &Image, threshold: u8) -> (Diff, Option<Image>) {
    if a.width != b.width || a.height != b.height {
        return (
            Diff {
                width: b.width,
                height: b.height,
                dimension_mismatch: true,
                changed_pixels: 0,
                changed_ratio: 1.0,
                bbox: None,
                mean_abs_delta: 0.0,
                max_delta: 255,
            },
            None,
        );
    }
    let (w, h) = (a.width as usize, a.height as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    let (mut changed, mut sum, mut max) = (0u64, 0u64, 0u8);
    let mut bbox: Option<[u32; 4]> = None;
    for y in 0..a.height {
        for x in 0..a.width {
            let i = (y as usize * w + x as usize) * 4;
            let d = (0..4)
                .map(|c| a.rgba[i + c].abs_diff(b.rgba[i + c]))
                .max()
                .unwrap_or(0);
            sum += u64::from(d);
            max = max.max(d);
            if d > threshold {
                changed += 1;
                bbox = Some(match bbox {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
                out.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                let g = b.rgba[i] / 4 + 160;
                out.extend_from_slice(&[g, g, g, 255]);
            }
        }
    }
    let total = (w * h).max(1) as f64;
    (
        Diff {
            width: a.width,
            height: a.height,
            dimension_mismatch: false,
            changed_pixels: changed,
            changed_ratio: changed as f64 / total,
            bbox,
            mean_abs_delta: sum as f64 / total,
            max_delta: max,
        },
        Some(Image {
            width: a.width,
            height: a.height,
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
pub fn list_pngs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
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
pub fn analyze_frames(dir: &Path, threshold: u8) -> Result<Frames, String> {
    let files = list_pngs(dir)?;
    if files.len() < 3 {
        return Err("need at least 3 PNG frames".into());
    }
    let images = files
        .iter()
        .map(|p| load(p))
        .collect::<Result<Vec<_>, _>>()?;
    analyze_images(&images, threshold)
}

/// # Errors
///
/// Returns an error if there are fewer than three frames or their dimensions differ.
pub fn analyze_images(images: &[Image], threshold: u8) -> Result<Frames, String> {
    let [first, _, .., last] = images else {
        return Err("need at least 3 frames".into());
    };
    let mut ratios = Vec::new();
    for pair in images.windows(2) {
        let (d, _) = diff_images(&pair[0], &pair[1], threshold);
        if d.dimension_mismatch {
            return Err("frames have different dimensions".into());
        }
        ratios.push(d.changed_ratio);
    }
    let eps = 0.0005;
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
            if r > 0.05 && r > 4.0 * median {
                jumps.push(frame);
            }
        }
        if s - f + 1 < 3 {
            notes.push(
                "the change spans fewer than 3 frames: it is effectively instant, not a tween"
                    .to_owned(),
            );
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

fn saturation(c: [u8; 3]) -> f64 {
    let (mx, mn) = (
        c.iter().max().copied().unwrap_or(0),
        c.iter().min().copied().unwrap_or(0),
    );
    if mx == 0 {
        0.0
    } else {
        f64::from(mx - mn) / f64::from(mx)
    }
}

pub fn palette(image: &Image, k: usize) -> Palette {
    // Histogram over 4 bits per channel, then greedy merge of nearby buckets.
    let mut buckets: std::collections::HashMap<u16, (u64, [u64; 3])> =
        std::collections::HashMap::new();
    let mut total = 0u64;
    for px in image.rgba.as_chunks::<4>().0 {
        if px[3] < 128 {
            continue;
        }
        total += 1;
        let key =
            (u16::from(px[0] >> 4) << 8) | (u16::from(px[1] >> 4) << 4) | u16::from(px[2] >> 4);
        let e = buckets.entry(key).or_insert((0, [0; 3]));
        e.0 += 1;
        for (sum, v) in e.1.iter_mut().zip(px) {
            *sum += u64::from(*v);
        }
    }
    let mut sorted: Vec<(u64, [f64; 3])> = buckets
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
    let mut clusters: Vec<(u64, [f64; 3])> = Vec::new();
    for (n, c) in sorted {
        let near = clusters.iter_mut().find(|(_, cc)| {
            ((cc[0] - c[0]).powi(2) + (cc[1] - c[1]).powi(2) + (cc[2] - c[2]).powi(2)).sqrt() < 48.0
        });
        match near {
            Some((cn, cc)) => {
                let t = (*cn + n) as f64;
                for i in 0..3 {
                    cc[i] = (cc[i] * *cn as f64 + c[i] * n as f64) / t;
                }
                *cn += n;
            }
            None => clusters.push((n, c)),
        }
    }
    clusters.sort_by_key(|b| std::cmp::Reverse(b.0));
    clusters.truncate(k.max(1));
    let colors: Vec<([u8; 3], f64)> = clusters
        .iter()
        .map(|(n, c)| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "RGB centroids are weighted averages of byte values and rounded to bytes"
            )]
            let rgb = c.map(|channel| channel.round() as u8);
            (rgb, *n as f64 / total.max(1) as f64)
        })
        .collect();
    let background = colors.first().map(|c| c.0);
    let text = background.and_then(|bg| {
        colors
            .iter()
            .filter(|c| c.1 >= 0.005)
            .map(|c| (c.0, contrast(c.0, bg)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    });
    let accent = colors
        .iter()
        .skip(1)
        .filter(|c| c.1 >= 0.002 && Some(c.0) != text.map(|t| t.0))
        .max_by(|a, b| saturation(a.0).total_cmp(&saturation(b.0)))
        .filter(|c| saturation(c.0) > 0.25)
        .map(|c| c.0);
    let surface = background.and_then(|bg| {
        colors
            .iter()
            .skip(1)
            .find(|c| contrast(c.0, bg) < 1.5 && c.1 >= 0.02)
            .map(|c| c.0)
    });
    Palette {
        suggested: Roles {
            background: background.map(hex),
            text: text.map(|t| hex(t.0)),
            text_contrast: text.map(|t| (t.1 * 100.0).round() / 100.0),
            accent: accent.map(hex),
            surface: surface.map(hex),
        },
        colors: colors
            .into_iter()
            .map(|(c, s)| Swatch {
                hex: hex(c),
                share: (s * 1000.0).round() / 1000.0,
            })
            .collect(),
        note: "Heuristic: the most common color is taken as the background, so verify roles against the reference by eye.",
    }
}

// ---------- contact sheet ----------
/// # Errors
///
/// Returns an error if the input count is outside 1–64, an image cannot be read, or the output dimensions or buffer exceed supported limits.
pub fn sheet(files: &[PathBuf], cols: usize, thumb_w: u32) -> Result<Image, String> {
    if files.is_empty() {
        return Err("no images".into());
    }
    if files.len() > 64 {
        return Err("at most 64 images per sheet".into());
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
            .map_err(|_| "thumbnail height exceeds u32")?;
            let mut px = Vec::new();
            for y in 0..th {
                let sy = usize::try_from(u64::from(y) * u64::from(im.height) / u64::from(th))
                    .map_err(|_| "thumbnail row is out of range")?;
                for x in 0..tw {
                    let sx = usize::try_from(u64::from(x) * u64::from(im.width) / u64::from(tw))
                        .map_err(|_| "thumbnail column is out of range")?;
                    let i = (sy * im.width as usize + sx) * 4;
                    px.extend_from_slice(&im.rgba[i..i + 4]);
                }
            }
            Ok(Image {
                width: tw,
                height: th,
                rgba: px,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let pad = 8u32;
    let rows = thumbs.len().div_ceil(cols);
    let cell_w = thumbs.iter().map(|t| t.width).max().unwrap_or(1);
    let cell_h = thumbs.iter().map(|t| t.height).max().unwrap_or(1);
    let dimension = |count, cell: u32| {
        let count = u32::try_from(count).ok()?;
        cell.checked_add(pad)?.checked_mul(count)?.checked_add(pad)
    };
    let w = dimension(cols, cell_w).ok_or("sheet width exceeds u32")?;
    let h = dimension(rows, cell_h).ok_or("sheet height exceeds u32")?;
    let len = (w as usize)
        .checked_mul(h as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("sheet buffer is too large")?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(len).map_err(|e| e.to_string())?;
    rgba.resize(len, 0);
    for px in rgba.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&[90, 90, 90, 255]);
    }
    let pad = pad as usize;
    for (n, t) in thumbs.iter().enumerate() {
        let ox = pad + (n % cols) * (cell_w as usize + pad);
        let oy = pad + (n / cols) * (cell_h as usize + pad);
        let stride = t.width as usize * 4;
        for (y, src) in t.rgba.chunks_exact(stride).enumerate() {
            let dst = ((oy + y) * w as usize + ox) * 4;
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
            entry.exit = Some(127);
            continue;
        };
        let start = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s.code(),
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Some(124);
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
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
        if n.interactive && (n.w < 24.0 || n.h < 24.0) {
            add(
                "target-size",
                &n.id,
                format!("{} x {} px is below 24 x 24 px", n.w, n.h),
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
        if let (Some(fg), Some(bg)) = (
            n.fg.as_deref().and_then(parse_hex),
            n.bg.as_deref().and_then(parse_hex),
        ) {
            let non_text = matches!(n.kind.as_deref(), Some("icon" | "boundary"));
            let px = n.font_px.unwrap_or(14.0);
            let large = px >= 24.0 || (n.bold && px >= 18.66);
            let need = if non_text || large { 3.0 } else { 4.5 };
            let got = contrast(fg, bg);
            if got + 1e-9 < need {
                add("contrast", &n.id, format!("{got:.2}:1, needs {need}:1"));
            }
        }
        if n.clipped {
            add("clipped", &n.id, "content is clipped".to_owned());
        }
        if n.x < -0.5
            || n.y < -0.5
            || n.x + n.w > tree.window.w + 0.5
            || n.y + n.h > tree.window.h + 0.5
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
        .take(2000)
        .collect();
    for (i, a) in interactive.iter().enumerate() {
        for b in &interactive[i + 1..] {
            let ix = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
            let iy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
            if ix > 0.0 && iy > 0.0 {
                let smaller = (a.w * a.h).min(b.w * b.h).max(1.0);
                if ix * iy / smaller > 0.25 {
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
            if d > 0.01 && d <= 3.0 {
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
