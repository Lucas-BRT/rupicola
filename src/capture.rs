//! Framework-agnostic offscreen RAM capture and ffmpeg MP4 encoding.
//!
//! Designed for Tier 1 background visual verification (e.g., `iced_tiny_skia`,
//! `libcosmic`, `slint` software renderer, `egui`) without opening a window,
//! starting Xvfb, or touching the user's live display.

use crate::visual::{self, Frames, Image};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Pixel layout produced by an offscreen renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PixelFormat {
    /// Straight (un-premultiplied) 8-bit RGBA.
    Rgba8,
    /// Premultiplied 8-bit RGBA (e.g., `tiny_skia::Pixmap::data()`).
    PremultipliedRgba8,
    /// Straight 8-bit BGRA.
    Bgra8,
    /// Premultiplied 8-bit BGRA.
    PremultipliedBgra8,
}

impl PixelFormat {
    /// # Errors
    ///
    /// Returns an error if the pixel format name is unsupported.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rgba" | "rgba8" => Ok(Self::Rgba8),
            "premultiplied-rgba" | "premultiplied-rgba8" | "pm-rgba" => {
                Ok(Self::PremultipliedRgba8)
            }
            "bgra" | "bgra8" => Ok(Self::Bgra8),
            "premultiplied-bgra" | "premultiplied-bgra8" | "pm-bgra" => {
                Ok(Self::PremultipliedBgra8)
            }
            other => Err(format!(
                "unsupported pixel format '{other}'; expected rgba, premultiplied-rgba, bgra, or premultiplied-bgra"
            )),
        }
    }

    /// Convert a raw pixel buffer in this format to straight 8-bit RGBA.
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer length is not a multiple of four.
    pub fn to_rgba8(self, src: &[u8]) -> Result<Vec<u8>, String> {
        if !src.len().is_multiple_of(4) {
            return Err(format!(
                "pixel buffer length {} is not a multiple of 4",
                src.len()
            ));
        }
        match self {
            Self::Rgba8 => Ok(src.to_vec()),
            Self::PremultipliedRgba8 => {
                let mut out = Vec::with_capacity(src.len());
                for px in src.as_chunks::<4>().0 {
                    let [r, g, b, a] = [px[0], px[1], px[2], px[3]];
                    out.extend_from_slice(&unpremultiply(r, g, b, a));
                }
                Ok(out)
            }
            Self::Bgra8 => {
                let mut out = Vec::with_capacity(src.len());
                for px in src.as_chunks::<4>().0 {
                    out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
                Ok(out)
            }
            Self::PremultipliedBgra8 => {
                let mut out = Vec::with_capacity(src.len());
                for px in src.as_chunks::<4>().0 {
                    let [b, g, r, a] = [px[0], px[1], px[2], px[3]];
                    out.extend_from_slice(&unpremultiply(r, g, b, a));
                }
                Ok(out)
            }
        }
    }
}

fn unpremultiply(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    if a == 0 {
        [0, 0, 0, 0]
    } else if a == 255 {
        [r, g, b, 255]
    } else {
        let a16 = u16::from(a);
        let half = a16 / 2;
        let conv = |c: u8| ((u16::from(c) * 255 + half) / a16).min(255) as u8;
        [conv(r), conv(g), conv(b), a]
    }
}

/// Configuration for a deterministic offscreen RAM capture sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: usize,
    pub pixel_format: PixelFormat,
}

impl CaptureConfig {
    /// # Errors
    ///
    /// Returns an error for zero dimensions or frames, a nonpositive or nonfinite frame rate, or dimensions that overflow the frame buffer size.
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("capture width and height must be greater than 0".into());
        }
        if !self.fps.is_finite() || self.fps <= 0.0 {
            return Err(format!(
                "capture fps must be a positive finite number, got {}",
                self.fps
            ));
        }
        if self.frame_count == 0 {
            return Err("capture frame_count must be greater than 0".into());
        }
        self.buffer_len()?;
        Ok(())
    }

    /// Deterministic timestamp in milliseconds for `frame` (0.0, 16.67, 33.33, ... at 60 fps).
    #[must_use]
    pub fn timestamp_ms(&self, frame: usize) -> f64 {
        frame as f64 * (1000.0 / self.fps)
    }

    /// Expected byte length of one raw frame buffer (`width * height * 4`).
    ///
    /// # Errors
    ///
    /// Returns an error if the frame buffer size overflows `usize`.
    pub fn buffer_len(&self) -> Result<usize, String> {
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|px| px.checked_mul(4))
            .ok_or_else(|| "capture dimensions overflow buffer size".into())
    }
}

/// Measured CPU rasterization timing for a single offscreen frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameTiming {
    pub frame: usize,
    pub timestamp_ms: f64,
    pub render_ms: f64,
}

/// Summary timing report for an offscreen capture sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureReport {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: usize,
    pub budget_ms: f64,
    pub total_render_ms: f64,
    pub mean_render_ms: f64,
    pub max_render_ms: f64,
    pub p95_render_ms: f64,
    pub over_budget_frames: Vec<usize>,
    pub frames: Vec<FrameTiming>,
}

/// Captured animation sequence held in RAM with per-frame CPU raster timings.
#[derive(Debug, Clone)]
pub struct RamCapture {
    pub frames: Vec<Image>,
    pub report: CaptureReport,
}

impl RamCapture {
    /// Render `config.frame_count` frames into RAM at deterministic timestamps
    /// (`0 ms`, `1000/fps ms`, ...) and record per-frame CPU rasterization time.
    ///
    /// `render_frame(frame_index, timestamp_ms, pixel_buffer)` writes one frame
    /// in `config.pixel_format` into the pre-allocated, zeroed `pixel_buffer`.
    ///
    /// # Errors
    ///
    /// Returns an error if the capture configuration is invalid or the frame renderer fails.
    pub fn record<F>(config: CaptureConfig, mut render_frame: F) -> Result<Self, String>
    where
        F: FnMut(usize, f64, &mut [u8]) -> Result<(), String>,
    {
        config.validate()?;
        let buf_len = config.buffer_len()?;
        let mut scratch = vec![0u8; buf_len];
        let mut images = Vec::with_capacity(config.frame_count);
        let mut timings = Vec::with_capacity(config.frame_count);

        for frame in 0..config.frame_count {
            scratch.fill(0);
            let timestamp_ms = config.timestamp_ms(frame);
            let start = Instant::now();
            render_frame(frame, timestamp_ms, &mut scratch)?;
            let render_ms = start.elapsed().as_secs_f64() * 1000.0;

            let rgba = config.pixel_format.to_rgba8(&scratch)?;
            images.push(Image {
                width: config.width,
                height: config.height,
                rgba,
            });
            timings.push(FrameTiming {
                frame,
                timestamp_ms,
                render_ms,
            });
        }

        let report = summarize_timings(&config, timings);
        Ok(Self {
            frames: images,
            report,
        })
    }

    /// Write lossless PNG frames (`frame_0000.png`, ...) and `timing.json` to `dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory, frame PNGs, or timing report cannot be created, or if serialization fails.
    pub fn save_pngs(&self, dir: &Path) -> Result<Vec<PathBuf>, String> {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut paths = Vec::with_capacity(self.frames.len());
        for (i, image) in self.frames.iter().enumerate() {
            let path = dir.join(format!("frame_{i:04}.png"));
            visual::save(&path, image)?;
            paths.push(path);
        }
        let timing_path = dir.join("timing.json");
        let timing_json = serde_json::to_string_pretty(&self.report).map_err(|e| e.to_string())?;
        fs::write(&timing_path, timing_json)
            .map_err(|e| format!("{}: {e}", timing_path.display()))?;
        Ok(paths)
    }

    /// Encode the RAM-buffered frames directly to a CFR MP4 via `ffmpeg`.
    ///
    /// # Errors
    ///
    /// Returns an error if the video options or frames are invalid or `ffmpeg` fails.
    pub fn encode_mp4(&self, out: &Path, options: &VideoOptions) -> Result<VideoReport, String> {
        encode_rgba_images(&self.frames, out, options)
    }

    /// Run `rupicola frames` motion analysis directly on the in-RAM frames.
    ///
    /// # Errors
    ///
    /// Returns an error if there are fewer than three frames or their dimensions differ.
    pub fn analyze(&self, threshold: u8) -> Result<Frames, String> {
        visual::analyze_images(&self.frames, threshold)
    }
}

fn summarize_timings(config: &CaptureConfig, frames: Vec<FrameTiming>) -> CaptureReport {
    let budget_ms = 1000.0 / config.fps;
    let total_render_ms: f64 = frames.iter().map(|t| t.render_ms).sum();
    let mean_render_ms = total_render_ms / frames.len() as f64;
    let max_render_ms = frames.iter().map(|t| t.render_ms).fold(0.0f64, f64::max);
    let mut sorted: Vec<f64> = frames.iter().map(|t| t.render_ms).collect();
    sorted.sort_by(f64::total_cmp);
    // ceil(0.95 * n) is n - floor(n / 20).
    let p95_idx = (sorted.len() - sorted.len() / 20).saturating_sub(1);
    let p95_render_ms = sorted[p95_idx];
    let over_budget_frames = frames
        .iter()
        .filter(|t| t.render_ms > budget_ms)
        .map(|t| t.frame)
        .collect();

    CaptureReport {
        width: config.width,
        height: config.height,
        fps: config.fps,
        frame_count: frames.len(),
        budget_ms,
        total_render_ms,
        mean_render_ms,
        max_render_ms,
        p95_render_ms,
        over_budget_frames,
        frames,
    }
}

/// Options for assembling frames into a constant-framerate MP4 with `ffmpeg`.
#[derive(Debug, Clone)]
pub struct VideoOptions {
    pub fps: f64,
    pub crf: u8,
    pub preset: String,
    pub ffmpeg_bin: String,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            fps: 60.0,
            crf: 18,
            preset: "fast".into(),
            ffmpeg_bin: "ffmpeg".into(),
        }
    }
}

impl VideoOptions {
    /// # Errors
    ///
    /// Returns an error if the frame rate is nonpositive or nonfinite, the CRF is out of range, or the preset is empty.
    pub fn validate(&self) -> Result<(), String> {
        if !self.fps.is_finite() || self.fps <= 0.0 {
            return Err(format!(
                "fps must be a positive finite number, got {}",
                self.fps
            ));
        }
        if self.crf > 51 {
            return Err(format!("crf must be in 0..=51, got {}", self.crf));
        }
        if self.preset.trim().is_empty() {
            return Err("x264 preset cannot be empty".into());
        }
        Ok(())
    }
}

/// Summary of an encoded MP4 video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoReport {
    pub out: PathBuf,
    pub width: u32,
    pub height: u32,
    pub encoded_width: u32,
    pub encoded_height: u32,
    pub fps: f64,
    pub frame_count: usize,
    pub duration_secs: f64,
    pub bytes: u64,
    pub saved_frames_dir: Option<PathBuf>,
}

/// Assemble a directory of PNG frames (in natural numeric order) into a CFR MP4.
///
/// # Errors
///
/// Returns an error if the directory has no PNGs, a frame cannot be read or has mismatched dimensions, or video encoding fails.
pub fn encode_png_dir(
    dir: &Path,
    out: &Path,
    options: &VideoOptions,
) -> Result<VideoReport, String> {
    options.validate()?;
    let files = visual::list_pngs(dir)?;
    if files.is_empty() {
        return Err(format!("{}: no PNG frames found", dir.display()));
    }
    let first = visual::load(&files[0])?;
    let (width, height) = (first.width, first.height);
    let mut encoder = FfmpegPipe::spawn(width, height, out, options)?;
    encoder.write_rgba(&first.rgba)?;
    for path in &files[1..] {
        let image = visual::load(path)?;
        if image.width != width || image.height != height {
            encoder.abort();
            return Err(format!(
                "{}: frame dimensions {}x{} do not match first frame {}x{}",
                path.display(),
                image.width,
                image.height,
                width,
                height
            ));
        }
        encoder.write_rgba(&image.rgba)?;
    }
    encoder.finish(files.len(), None)
}

/// Assemble in-RAM RGBA `Image` frames into a CFR MP4 via `ffmpeg`.
///
/// # Errors
///
/// Returns an error if the options are invalid, there are no frames, frame dimensions differ, or video encoding fails.
pub fn encode_rgba_images(
    images: &[Image],
    out: &Path,
    options: &VideoOptions,
) -> Result<VideoReport, String> {
    options.validate()?;
    let first = images
        .first()
        .ok_or_else(|| "cannot encode empty frame sequence".to_owned())?;
    let (width, height) = (first.width, first.height);
    let mut encoder = FfmpegPipe::spawn(width, height, out, options)?;
    for (idx, image) in images.iter().enumerate() {
        if image.width != width || image.height != height {
            encoder.abort();
            return Err(format!(
                "frame {idx} dimensions {}x{} do not match first frame {}x{}",
                image.width, image.height, width, height
            ));
        }
        encoder.write_rgba(&image.rgba)?;
    }
    encoder.finish(images.len(), None)
}

/// Stream raw RGBA/BGRA frames (e.g., from RAM via stdin) into a CFR MP4 via `ffmpeg`,
/// optionally also writing lossless PNGs to `save_frames`.
///
/// # Errors
///
/// Returns an error if the options or frame dimensions are invalid, the stream cannot be read or has no complete frames, or video encoding fails.
pub fn encode_raw_stream<R: Read>(
    mut reader: R,
    width: u32,
    height: u32,
    format: PixelFormat,
    out: &Path,
    save_frames: Option<&Path>,
    options: &VideoOptions,
) -> Result<VideoReport, String> {
    options.validate()?;
    if width == 0 || height == 0 {
        return Err("video width and height must be greater than 0".into());
    }
    let frame_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "video dimensions overflow frame buffer size".to_owned())?;

    if let Some(dir) = save_frames {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    let mut encoder = FfmpegPipe::spawn(width, height, out, options)?;
    let mut buf = vec![0u8; frame_bytes];
    let mut frame_count = 0usize;

    loop {
        match read_exact_or_eof(&mut reader, &mut buf)? {
            ReadFrame::Eof => break,
            ReadFrame::Partial(n) => {
                encoder.abort();
                return Err(format!(
                    "truncated raw frame {frame_count}: read {n} of {frame_bytes} bytes"
                ));
            }
            ReadFrame::Full => {
                let rgba = format.to_rgba8(&buf)?;
                if let Some(dir) = save_frames {
                    let img = Image {
                        width,
                        height,
                        rgba: rgba.clone(),
                    };
                    visual::save(&dir.join(format!("frame_{frame_count:04}.png")), &img)?;
                }
                encoder.write_rgba(&rgba)?;
                frame_count += 1;
            }
        }
    }

    if frame_count == 0 {
        encoder.abort();
        return Err("no raw frames received on input stream".into());
    }

    encoder.finish(frame_count, save_frames.map(Path::to_path_buf))
}

enum ReadFrame {
    Eof,
    Full,
    Partial(usize),
}

fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<ReadFrame, String> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                return Ok(if filled == 0 {
                    ReadFrame::Eof
                } else {
                    ReadFrame::Partial(filled)
                });
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("failed reading raw frame stream: {e}")),
        }
    }
    Ok(ReadFrame::Full)
}

struct FfmpegPipe {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    out: PathBuf,
    width: u32,
    height: u32,
    fps: f64,
}

impl FfmpegPipe {
    fn spawn(width: u32, height: u32, out: &Path, options: &VideoOptions) -> Result<Self, String> {
        if let Some(parent) = out.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let fps_str = format_fps(options.fps);
        let size_str = format!("{width}x{height}");
        let crf_str = options.crf.to_string();

        let mut cmd = Command::new(&options.ffmpeg_bin);
        cmd.args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &size_str,
            "-framerate",
            &fps_str,
            "-i",
            "pipe:0",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-r",
            &fps_str,
            "-fps_mode",
            "cfr",
            "-crf",
            &crf_str,
            "-preset",
            &options.preset,
            "-movflags",
            "+faststart",
        ])
        .arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                format!(
                    "'{}' not found in PATH; install ffmpeg (e.g. `sudo apt install ffmpeg` or `brew install ffmpeg`) to encode MP4 videos",
                    options.ffmpeg_bin
                )
            } else {
                format!("failed to start '{}': {e}", options.ffmpeg_bin)
            }
        })?;
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            out: out.to_path_buf(),
            width,
            height,
            fps: options.fps,
        })
    }

    fn write_rgba(&mut self, rgba: &[u8]) -> Result<(), String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err("ffmpeg stdin already closed".into());
        };
        if let Err(write_err) = stdin.write_all(rgba) {
            self.stdin.take();
            let _ = self.child.wait();
            let detail = self
                .child
                .stderr
                .take()
                .and_then(|mut s| {
                    let mut buf = String::new();
                    s.read_to_string(&mut buf).ok().map(|_| buf)
                })
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());
            return Err(match detail {
                Some(stderr) => format!("ffmpeg failed while writing frame: {stderr}"),
                None => format!("ffmpeg pipe closed while writing frame: {write_err}"),
            });
        }
        Ok(())
    }

    fn abort(mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn finish(
        mut self,
        frame_count: usize,
        saved_frames_dir: Option<PathBuf>,
    ) -> Result<VideoReport, String> {
        self.stdin.take();
        let output = self
            .child
            .wait_with_output()
            .map_err(|e| format!("failed waiting for ffmpeg: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if stderr.is_empty() {
                format!("ffmpeg exited with status {}", output.status)
            } else {
                format!("ffmpeg failed: {stderr}")
            });
        }
        let bytes = File::open(&self.out)
            .and_then(|f| f.metadata())
            .map(|m| m.len())
            .map_err(|e| format!("{}: {e}", self.out.display()))?;
        Ok(VideoReport {
            out: self.out,
            width: self.width,
            height: self.height,
            encoded_width: self.width + (self.width % 2),
            encoded_height: self.height + (self.height % 2),
            fps: self.fps,
            frame_count,
            duration_secs: frame_count as f64 / self.fps,
            bytes,
            saved_frames_dir,
        })
    }
}

fn format_fps(fps: f64) -> String {
    if (fps.fract()).abs() < 1e-9 {
        format!("{:.0}", fps.trunc())
    } else {
        format!("{fps:.6}")
    }
}
