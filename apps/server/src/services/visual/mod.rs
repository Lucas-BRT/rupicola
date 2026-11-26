//! Pure-Rust PNG visual diffing and region cropping (`VisualDiffOps`).

use crate::{
    domain::{ApiError, Principal, secret::now_ms},
    services::{captures, publish},
    state::Shared,
};
use base64::Engine as _;
use rupicola_api::{
    ArtifactChunk, CaptureStatus, CropRect, VisualDiffMode, VisualDiffReport,
    record::{
        CaptureRecord, Environment as RecordEnvironment, Host, Member, MemberRole, ParentKind,
        ParentRef, Platform, PlatformFamily, RECORD_FORMAT, RecordKind, ScenarioRef, Software,
    },
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _, io::Cursor};
use uuid::Uuid;

const PIXEL_DIFF_THRESHOLD: u8 = 8;
const TILE_SIZE: u32 = 32;
const MAX_PNG_BYTES: usize = 32 * 1024 * 1024;
/// Maximum decoded pixel count (8000x8000 = 256 MiB RGBA) to block decompression bombs.
const MAX_PNG_PIXELS: u64 = 64_000_000;

struct RgbaImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn decode_rgba(bytes: &[u8]) -> Result<RgbaImage, ApiError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| ApiError::invalid(format!("invalid PNG: {e}")))?;
    let info = reader.info();
    let pixels = u64::from(info.width).saturating_mul(u64::from(info.height));
    if pixels == 0 || pixels > MAX_PNG_PIXELS {
        return Err(ApiError::invalid(format!(
            "PNG dimensions {}x{} exceed safety limit ({MAX_PNG_PIXELS} pixels)",
            info.width, info.height
        )));
    }
    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| ApiError::invalid("PNG output buffer size overflow"))?;
    let mut buf = vec![0; buf_size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| ApiError::invalid(format!("failed to decode PNG frame: {e}")))?;
    let raw = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => raw
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => raw
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => raw.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => {
            return Err(ApiError::invalid("unexpected indexed PNG after expand"));
        }
    };
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        data: rgba,
    })
}

fn encode_rgba(img: &RgbaImage) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, img.width, img.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| ApiError::invalid(format!("PNG header encode error: {e}")))?;
        writer
            .write_image_data(&img.data)
            .map_err(|e| ApiError::invalid(format!("PNG write error: {e}")))?;
    }
    Ok(out)
}

fn pixel_at(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    if x >= img.width || y >= img.height {
        return [0, 0, 0, 0];
    }
    let idx = ((y as usize) * (img.width as usize) + (x as usize)) * 4;
    [
        img.data[idx],
        img.data[idx + 1],
        img.data[idx + 2],
        img.data[idx + 3],
    ]
}

fn max_channel_diff(a: [u8; 4], b: [u8; 4]) -> u8 {
    a[0].abs_diff(b[0])
        .max(a[1].abs_diff(b[1]))
        .max(a[2].abs_diff(b[2]))
        .max(a[3].abs_diff(b[3]))
}

#[allow(clippy::cast_precision_loss)]
fn compute_diff(
    before: &RgbaImage,
    after: &RgbaImage,
    mode: VisualDiffMode,
) -> (RgbaImage, f64, Vec<CropRect>) {
    let w = before.width.max(after.width);
    let h = before.height.max(after.height);
    let out_w = match mode {
        VisualDiffMode::SideBySide => w * 2,
        VisualDiffMode::Highlight | VisualDiffMode::Heatmap => w,
    };
    let mut out = vec![0u8; (out_w as usize) * (h as usize) * 4];
    let tiles_x = w.div_ceil(TILE_SIZE) as usize;
    let tiles_y = h.div_ceil(TILE_SIZE) as usize;
    let mut dirty_tiles = vec![false; tiles_x * tiles_y];
    let mut changed_pixels: u64 = 0;

    for y in 0..h {
        for x in 0..w {
            let pa = pixel_at(before, x, y);
            let pb = pixel_at(after, x, y);
            let delta = max_channel_diff(pa, pb);
            if delta > PIXEL_DIFF_THRESHOLD {
                changed_pixels += 1;
                let tx = (x / TILE_SIZE) as usize;
                let ty = (y / TILE_SIZE) as usize;
                dirty_tiles[ty * tiles_x + tx] = true;
            }
            match mode {
                VisualDiffMode::SideBySide => {
                    let left = ((y as usize) * (out_w as usize) + (x as usize)) * 4;
                    let right = ((y as usize) * (out_w as usize) + ((x + w) as usize)) * 4;
                    out[left..left + 4].copy_from_slice(&pa);
                    out[right..right + 4].copy_from_slice(&pb);
                }
                VisualDiffMode::Highlight => {
                    let idx = ((y as usize) * (out_w as usize) + (x as usize)) * 4;
                    if delta > PIXEL_DIFF_THRESHOLD {
                        out[idx..idx + 4].copy_from_slice(&[235, 64, 52, 255]);
                    } else {
                        out[idx..idx + 4].copy_from_slice(&[pb[0] / 3, pb[1] / 3, pb[2] / 3, 255]);
                    }
                }
                VisualDiffMode::Heatmap => {
                    let idx = ((y as usize) * (out_w as usize) + (x as usize)) * 4;
                    let intensity = delta;
                    out[idx..idx + 4].copy_from_slice(&[
                        intensity,
                        255u8.saturating_sub(intensity),
                        40,
                        255,
                    ]);
                }
            }
        }
    }

    let regions = collect_regions(&dirty_tiles, tiles_x, tiles_y, w, h);
    let total = u64::from(w) * u64::from(h);
    let ratio = if total == 0 {
        0.0
    } else {
        (changed_pixels as f64) / (total as f64)
    };
    (
        RgbaImage {
            width: out_w,
            height: h,
            data: out,
        },
        ratio,
        regions,
    )
}

fn collect_regions(
    dirty: &[bool],
    tiles_x: usize,
    tiles_y: usize,
    w: u32,
    h: u32,
) -> Vec<CropRect> {
    let mut visited = vec![false; dirty.len()];
    let mut regions = Vec::new();
    for sy in 0..tiles_y {
        for sx in 0..tiles_x {
            let start = sy * tiles_x + sx;
            if !dirty[start] || visited[start] {
                continue;
            }
            let mut stack = vec![(sx, sy)];
            visited[start] = true;
            let (mut min_x, mut max_x, mut min_y, mut max_y) = (sx, sx, sy, sy);
            while let Some((cx, cy)) = stack.pop() {
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < tiles_x && ny < tiles_y {
                        let ni = ny * tiles_x + nx;
                        if dirty[ni] && !visited[ni] {
                            visited[ni] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            let x = u32::try_from(min_x).unwrap_or(0) * TILE_SIZE;
            let y = u32::try_from(min_y).unwrap_or(0) * TILE_SIZE;
            let x2 = ((u32::try_from(max_x).unwrap_or(0) + 1) * TILE_SIZE).min(w);
            let y2 = ((u32::try_from(max_y).unwrap_or(0) + 1) * TILE_SIZE).min(h);
            regions.push(CropRect {
                x,
                y,
                width: x2.saturating_sub(x),
                height: y2.saturating_sub(y),
            });
        }
    }
    regions
}

/// Compares two PNG artifacts in the same project, stores the rendered diff PNG as a
/// derived capture artifact, and returns its metrics and changed bounding boxes.
pub async fn diff_images(
    state: &Shared,
    principal: &Principal,
    before_id: &str,
    after_id: &str,
    mode: VisualDiffMode,
) -> Result<VisualDiffReport, ApiError> {
    let (before_blob, before_bytes) =
        captures::read_artifact_range(state, principal, before_id, 0, MAX_PNG_BYTES).await?;
    let (after_blob, after_bytes) =
        captures::read_artifact_range(state, principal, after_id, 0, MAX_PNG_BYTES).await?;
    if before_blob.project_id != after_blob.project_id {
        return Err(ApiError::invalid(
            "before and after artifacts must belong to the same project",
        ));
    }

    let (diff_png, changed_ratio, changed_regions) = tokio::task::spawn_blocking(move || {
        let before = decode_rgba(&before_bytes)?;
        let after = decode_rgba(&after_bytes)?;
        let (diff_img, ratio, regions) = compute_diff(&before, &after, mode);
        let png_bytes = encode_rgba(&diff_img)?;
        Ok::<_, ApiError>((png_bytes, ratio, regions))
    })
    .await
    .map_err(anyhow::Error::from)??;

    let project = before_blob.project_id;
    let sha = sha256_hex(&diff_png);
    let len = u64::try_from(diff_png.len()).map_err(anyhow::Error::from)?;
    captures::put_project_blob(state, &project, sha.clone(), diff_png).await?;

    let cap_id = Uuid::now_v7().to_string();
    let record = CaptureRecord {
        format: RECORD_FORMAT,
        id: cap_id.clone(),
        project: project.clone(),
        kind: RecordKind::Derived,
        scenario: ScenarioRef {
            name: "visual-diff".to_owned(),
            sha256: None,
        },
        revision: None,
        series: "visual-diff@default@default".to_owned(),
        parents: vec![
            ParentRef {
                id: before_blob.capture_id,
                kind: ParentKind::Baseline,
            },
            ParentRef {
                id: after_blob.capture_id,
                kind: ParentKind::Input,
            },
        ],
        captured_at: now_ms(),
        status: CaptureStatus::Complete,
        blocker: None,
        environment: RecordEnvironment {
            platform: Some(Platform {
                family: Some(PlatformFamily::Web),
                os_version: None,
            }),
            display: None,
            appearance: None,
            software: Some(Software {
                backend: Some("rupicola-visual-diff".to_owned()),
                renderer: None,
                app_version: None,
                app_build: None,
            }),
            host: Some(Host {
                os: Some(std::env::consts::OS.to_owned()),
            }),
            device: None,
            extra: BTreeMap::new(),
        },
        plan_spec: None,
        members: vec![Member {
            path: "checkpoints/diff.png".to_owned(),
            role: MemberRole::Checkpoint,
            mime: "image/png".to_owned(),
            sha256: sha,
            bytes: len,
            owner: "diff".to_owned(),
            target_ms: None,
            video_ms: None,
            surface: None,
            locator: BTreeMap::new(),
        }],
    };
    let published = publish::publish(state, principal, &project, &cap_id, &record).await?;
    let cap = match published {
        publish::Published::New(c) | publish::Published::Same(c) => c,
    };
    let diff_artifact_id = cap
        .checkpoints
        .first()
        .map(|c| c.artifact.id.clone())
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("derived diff missing artifact")))?;

    Ok(VisualDiffReport {
        before_artifact_id: before_id.to_owned(),
        after_artifact_id: after_id.to_owned(),
        diff_artifact_id,
        changed_ratio,
        changed_regions,
    })
}

/// Crops a rectangular region from a PNG artifact and returns it as a base64 [`ArtifactChunk`].
pub async fn crop_region(
    state: &Shared,
    principal: &Principal,
    artifact_id: &str,
    rect: &CropRect,
) -> Result<ArtifactChunk, ApiError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(ApiError::invalid("crop width and height must be > 0"));
    }
    let (_, bytes) =
        captures::read_artifact_range(state, principal, artifact_id, 0, MAX_PNG_BYTES).await?;
    let rect = rect.clone();
    let cropped_png = tokio::task::spawn_blocking(move || {
        let img = decode_rgba(&bytes)?;
        let x0 = rect.x.min(img.width.saturating_sub(1));
        let y0 = rect.y.min(img.height.saturating_sub(1));
        let w = rect.width.min(img.width.saturating_sub(x0)).max(1);
        let h = rect.height.min(img.height.saturating_sub(y0)).max(1);
        let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
        for row in 0..h {
            let src_start = (((y0 + row) as usize) * (img.width as usize) + (x0 as usize)) * 4;
            let dst_start = (row as usize) * (w as usize) * 4;
            let row_bytes = (w as usize) * 4;
            out[dst_start..dst_start + row_bytes]
                .copy_from_slice(&img.data[src_start..src_start + row_bytes]);
        }
        encode_rgba(&RgbaImage {
            width: w,
            height: h,
            data: out,
        })
    })
    .await
    .map_err(anyhow::Error::from)??;

    let total = u64::try_from(cropped_png.len()).map_err(anyhow::Error::from)?;
    let len = cropped_png.len();
    Ok(ArtifactChunk {
        id: artifact_id.to_owned(),
        mime: "image/png".to_owned(),
        bytes: total,
        offset: 0,
        length: len,
        data: base64::engine::general_purpose::STANDARD.encode(cropped_png),
    })
}

#[cfg(test)]
mod tests;
