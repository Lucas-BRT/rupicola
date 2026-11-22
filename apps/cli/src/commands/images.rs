//! Image commands: diff, frames, video, palette and contact sheets.

use crate::{
    DEFAULT_PALETTE_COLORS, DEFAULT_SHEET_COLUMNS, DEFAULT_SHEET_THUMB_WIDTH, EXIT_CLEAN,
    EXIT_FINDINGS, PERCENT, Result, anyhow, bail, capture, json, visual,
};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct DiffArgs {
    before: PathBuf,
    after: PathBuf,
    /// Largest per-channel difference still treated as unchanged.
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    /// Write a diff image (changed pixels in red).
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

impl DiffArgs {
    pub(super) fn run(self) -> Result<u8> {
        let DiffArgs {
            before,
            after,
            threshold,
            out,
            json: output_json,
        } = self;
        let (a, b) = (visual::load(&before)?, visual::load(&after)?);
        let (report, image) = visual::diff_images(&a, &b, threshold);
        if let (Some(path), Some(image)) = (&out, &image) {
            visual::save(path, image)?;
        }
        if output_json {
            json(&report)?;
        } else if report.dimension_mismatch {
            println!(
                "different dimensions: {}x{} vs {}x{}",
                a.width, a.height, b.width, b.height
            );
        } else {
            println!(
                "{} of {} pixels changed ({:.2}%); mean delta {:.2}; max delta {}; region {:?}",
                report.changed_pixels,
                u64::from(report.width) * u64::from(report.height),
                report.changed_ratio * PERCENT,
                report.mean_abs_delta,
                report.max_delta,
                report.bbox
            );
        }
        Ok(if report.dimension_mismatch || report.changed_pixels > 0 {
            EXIT_FINDINGS
        } else {
            EXIT_CLEAN
        })
    }
}

#[derive(clap::Args)]
pub struct FramesArgs {
    dir: PathBuf,
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    #[arg(long)]
    json: bool,
}

impl FramesArgs {
    pub(super) fn run(self) -> Result<u8> {
        let FramesArgs {
            dir,
            threshold,
            json: output_json,
        } = self;
        let report = visual::analyze_frames(&dir, threshold)?;
        if output_json {
            json(&report)?;
        } else {
            println!(
                "{} frames; changed from {:?} to {:?}; stalls {:?}; jumps {:?}; first-to-last {:.2}% changed",
                report.frame_count,
                report.first_change,
                report.settled_at,
                report.stalls,
                report.jumps,
                report.total_changed_ratio * PERCENT
            );
            for note in &report.notes {
                println!("note: {note}");
            }
        }
        Ok(
            if report.stalls.is_empty() && report.jumps.is_empty() && report.first_change.is_some()
            {
                EXIT_CLEAN
            } else {
                EXIT_FINDINGS
            },
        )
    }
}

#[derive(clap::Args)]
pub struct VideoArgs {
    /// Directory of PNG frames (omit when using --raw to read pixels from stdin).
    dir: Option<PathBuf>,
    /// Output MP4 file path.
    #[arg(long)]
    out: PathBuf,
    /// Target constant frame rate.
    #[arg(long, default_value_t = capture::DEFAULT_VIDEO_FPS)]
    fps: f64,
    /// Read raw pixel frames from stdin instead of a PNG directory.
    #[arg(long)]
    raw: bool,
    /// Frame width in pixels (required with --raw).
    #[arg(long)]
    width: Option<u32>,
    /// Frame height in pixels (required with --raw).
    #[arg(long)]
    height: Option<u32>,
    /// Pixel format for --raw input: rgba, premultiplied-rgba, bgra, premultiplied-bgra.
    #[arg(long, default_value = "rgba")]
    pixel_format: String,
    /// Also save lossless PNG frames to this directory when reading --raw stdin.
    #[arg(long)]
    save_frames: Option<PathBuf>,
    /// x264 CRF quality (lower is sharper; see `capture::MAX_CRF` for the top).
    #[arg(long, default_value_t = capture::DEFAULT_CRF)]
    crf: u8,
    /// x264 encoder preset (e.g. ultrafast, fast, medium).
    #[arg(long, default_value = "fast")]
    preset: String,
    #[arg(long)]
    json: bool,
}

impl VideoArgs {
    pub(super) fn run(self) -> Result<u8> {
        let VideoArgs {
            dir,
            out,
            fps,
            raw,
            width,
            height,
            pixel_format,
            save_frames,
            crf,
            preset,
            json: output_json,
        } = self;
        let options = capture::VideoOptions {
            fps,
            crf,
            preset,
            ..capture::VideoOptions::default()
        };
        let report = if raw {
            if dir.is_some() {
                bail!("cannot combine a PNG directory with --raw");
            }
            let width = width.ok_or_else(|| anyhow!("--width is required with --raw"))?;
            let height = height.ok_or_else(|| anyhow!("--height is required with --raw"))?;
            let format = capture::PixelFormat::parse(&pixel_format)?;
            let stdin = std::io::stdin();
            capture::encode_raw_stream(
                stdin.lock(),
                width,
                height,
                format,
                &out,
                save_frames.as_deref(),
                &options,
            )?
        } else {
            let dir = dir.ok_or_else(|| {
                anyhow!("provide a PNG directory, or pass --raw to read from stdin")
            })?;
            if save_frames.is_some() {
                bail!("--save-frames is only valid with --raw");
            }
            capture::encode_png_dir(&dir, &out, &options)?
        };
        if output_json {
            json(&report)?;
        } else {
            println!(
                "{} ({} frames, {}x{} -> {}x{}, {:.2} fps, {:.2}s, {} bytes)",
                report.out.display(),
                report.frame_count,
                report.width,
                report.height,
                report.encoded_width,
                report.encoded_height,
                report.fps,
                report.duration_secs,
                report.bytes
            );
        }
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct PaletteArgs {
    image: PathBuf,
    #[arg(long, default_value_t = DEFAULT_PALETTE_COLORS)]
    colors: usize,
    #[arg(long)]
    json: bool,
}

impl PaletteArgs {
    pub(super) fn run(self) -> Result<u8> {
        let PaletteArgs {
            image,
            colors,
            json: output_json,
        } = self;
        let report = visual::palette(&visual::load(&image)?, colors);
        if output_json {
            json(&report)?;
        } else {
            for c in &report.colors {
                println!("{} {:.1}%", c.hex, c.share * PERCENT);
            }
            println!("suggested: {:?}", report.suggested);
        }
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct SheetArgs {
    dir: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = DEFAULT_SHEET_COLUMNS)]
    cols: usize,
    #[arg(long, default_value_t = DEFAULT_SHEET_THUMB_WIDTH)]
    thumb: u32,
}

impl SheetArgs {
    pub(super) fn run(self) -> Result<u8> {
        let SheetArgs {
            dir,
            out,
            cols,
            thumb,
        } = self;
        let files = visual::list_pngs(&dir)?;
        visual::save(&out, &visual::sheet(&files, cols, thumb)?)?;
        println!("{} images, in order:", files.len());
        for f in &files {
            println!("{}", f.display());
        }
        Ok(EXIT_CLEAN)
    }
}
