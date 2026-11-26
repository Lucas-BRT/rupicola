//! Standardized, token-efficient video & media inspection (`MediaInspectOps`).
//!
//! Ladder: `probe` (0 images) → `motion_diff` (JSON spikes) → `contact_sheet`
//! (1 image) → `sample_frames` (Coarse → Bisect → Window without re-reading frames).

use crate::{
    domain::{ApiError, Principal, secret::now_ms},
    services::{captures, publish},
    state::Shared,
};
use base64::Engine as _;
use rupicola_api::{
    ArtifactChunk, CaptureStatus, ContactSheetRequest, FrameSampleStrategy, MediaProbe,
    MotionDiffReport, MotionSpike, SampledFrame, SampledFrames,
    record::{
        CaptureRecord, Environment as RecordEnvironment, Host, Member, MemberRole, ParentKind,
        ParentRef, Platform, PlatformFamily, RECORD_FORMAT, RecordKind, ScenarioRef, Software,
    },
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::{Cursor, Write},
    path::Path,
    process::Command,
};
use uuid::Uuid;

const MAX_MEDIA_BYTES: usize = 128 * 1024 * 1024;
const MOTION_WIDTH: usize = 160;
const MOTION_PIXEL_DELTA: u8 = 12;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn write_temp_media(bytes: &[u8], mime: &str) -> Result<tempfile::NamedTempFile, ApiError> {
    let ext = match mime {
        "video/webm" => ".webm",
        "image/png" => ".png",
        _ => ".mp4",
    };
    let mut file = tempfile::Builder::new()
        .suffix(ext)
        .tempfile()
        .map_err(anyhow::Error::from)?;
    file.write_all(bytes).map_err(anyhow::Error::from)?;
    file.flush().map_err(anyhow::Error::from)?;
    Ok(file)
}

fn parse_rate(raw: &str) -> Option<f64> {
    let (num, den) = raw.split_once('/')?;
    let n: f64 = num.trim().parse().ok()?;
    let d: f64 = den.trim().parse().ok()?;
    (d > 0.0).then_some(n / d)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn probe_video_path(path: &Path, artifact_id: &str, mime: &str) -> Result<MediaProbe, ApiError> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|e| ApiError::invalid(format!("ffprobe unavailable: {e}")))?;
    if !out.status.success() {
        return Err(ApiError::invalid(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| ApiError::invalid(format!("ffprobe JSON parse error: {e}")))?;
    let video = parsed["streams"]
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["codec_type"] == "video"))
        .ok_or_else(|| ApiError::invalid("no video stream found in artifact"))?;

    let width = u32::try_from(video["width"].as_u64().unwrap_or(0)).unwrap_or(0);
    let height = u32::try_from(video["height"].as_u64().unwrap_or(0)).unwrap_or(0);
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(parse_rate)
        .or_else(|| video["r_frame_rate"].as_str().and_then(parse_rate));
    let duration_secs: Option<f64> = video["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            parsed["format"]["duration"]
                .as_str()
                .and_then(|s| s.parse().ok())
        });
    let duration_ms = duration_secs.map(|s| (s * 1000.0).round() as u64);
    let frame_count = video["nb_frames"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| Some((duration_secs? * fps?).round() as u64));
    let codec = video["codec_name"].as_str().map(str::to_owned);

    Ok(MediaProbe {
        artifact_id: artifact_id.to_owned(),
        mime: mime.to_owned(),
        width,
        height,
        duration_ms,
        fps,
        frame_count,
        codec,
    })
}

/// Probes image or video metadata without decoding frames into agent context.
pub async fn probe(
    state: &Shared,
    principal: &Principal,
    artifact_id: &str,
) -> Result<MediaProbe, ApiError> {
    let (blob, bytes) =
        captures::read_artifact_range(state, principal, artifact_id, 0, MAX_MEDIA_BYTES).await?;
    let id = artifact_id.to_owned();
    tokio::task::spawn_blocking(move || {
        if blob.mime == "image/png" {
            let decoder = png::Decoder::new(Cursor::new(&bytes));
            let reader = decoder
                .read_info()
                .map_err(|e| ApiError::invalid(format!("invalid PNG: {e}")))?;
            let info = reader.info();
            return Ok(MediaProbe {
                artifact_id: id,
                mime: blob.mime,
                width: info.width,
                height: info.height,
                duration_ms: None,
                fps: None,
                frame_count: Some(1),
                codec: Some("png".to_owned()),
            });
        }
        let tmp = write_temp_media(&bytes, &blob.mime)?;
        probe_video_path(tmp.path(), &id, &blob.mime)
    })
    .await
    .map_err(anyhow::Error::from)?
}

/// Computes frame-to-frame pixel delta & luma shift numerically to pinpoint flashes/jumps
/// as pure JSON before opening any PNGs.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub async fn motion_diff(
    state: &Shared,
    principal: &Principal,
    artifact_id: &str,
    threshold: f64,
) -> Result<MotionDiffReport, ApiError> {
    let (blob, bytes) =
        captures::read_artifact_range(state, principal, artifact_id, 0, MAX_MEDIA_BYTES).await?;
    let id = artifact_id.to_owned();
    let clamped_threshold = threshold.clamp(0.0, 1.0);
    tokio::task::spawn_blocking(move || {
        let tmp = write_temp_media(&bytes, &blob.mime)?;
        let info = probe_video_path(tmp.path(), &id, &blob.mime)?;
        if info.width == 0 || info.height == 0 {
            return Err(ApiError::invalid("video has zero dimensions"));
        }
        let scaled_h =
            (((u64::from(info.height) * (MOTION_WIDTH as u64)) / u64::from(info.width)) / 2 * 2)
                .max(2) as usize;
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(tmp.path())
            .args([
                "-vf",
                &format!("scale={MOTION_WIDTH}:{scaled_h}"),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-",
            ])
            .output()
            .map_err(|e| ApiError::invalid(format!("ffmpeg unavailable: {e}")))?;
        if !out.status.success() {
            return Err(ApiError::invalid(format!(
                "ffmpeg motion decode failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let frame_bytes = MOTION_WIDTH * scaled_h * 3;
        let pixels = (MOTION_WIDTH * scaled_h) as f64;
        let fps = info.fps.unwrap_or(60.0).max(1.0);
        let frames: Vec<&[u8]> = out.stdout.chunks_exact(frame_bytes).collect();
        let mut spikes = Vec::new();
        for (idx, pair) in frames.windows(2).enumerate() {
            let (prev, curr) = (pair[0], pair[1]);
            let mut changed = 0usize;
            let mut luma_prev = 0f64;
            let mut luma_curr = 0f64;
            for (p, c) in prev
                .as_chunks::<3>()
                .0
                .iter()
                .zip(curr.as_chunks::<3>().0.iter())
            {
                if p[0]
                    .abs_diff(c[0])
                    .max(p[1].abs_diff(c[1]))
                    .max(p[2].abs_diff(c[2]))
                    > MOTION_PIXEL_DELTA
                {
                    changed += 1;
                }
                luma_prev +=
                    0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
                luma_curr +=
                    0.2126 * f64::from(c[0]) + 0.7152 * f64::from(c[1]) + 0.0722 * f64::from(c[2]);
            }
            let delta_ratio = (changed as f64) / pixels;
            let luma_delta = ((luma_curr - luma_prev) / pixels) / 255.0;
            if delta_ratio >= clamped_threshold || luma_delta.abs() >= clamped_threshold {
                let frame_index = (idx as u64) + 1;
                spikes.push(MotionSpike {
                    frame_index,
                    timestamp_ms: (((frame_index as f64) / fps) * 1000.0).round() as u64,
                    delta_ratio,
                    luma_delta,
                });
            }
        }
        Ok(MotionDiffReport {
            artifact_id: id,
            threshold: clamped_threshold,
            spikes,
        })
    })
    .await
    .map_err(anyhow::Error::from)?
}

/// Renders one contact-sheet PNG (`cols x rows`) for 1-image visual triage.
#[allow(clippy::cast_precision_loss)]
pub async fn contact_sheet(
    state: &Shared,
    principal: &Principal,
    artifact_id: &str,
    req: &ContactSheetRequest,
) -> Result<ArtifactChunk, ApiError> {
    let cols = req.cols.clamp(1, 10);
    let rows = req.rows.clamp(1, 10);
    let slots = cols * rows;
    let (blob, bytes) =
        captures::read_artifact_range(state, principal, artifact_id, 0, MAX_MEDIA_BYTES).await?;
    let id = artifact_id.to_owned();
    let start_ms = req.start_ms;
    let end_ms = req.end_ms;
    let png_bytes = tokio::task::spawn_blocking(move || {
        let tmp = write_temp_media(&bytes, &blob.mime)?;
        let info = probe_video_path(tmp.path(), &id, &blob.mime)?;
        let total_ms = info.duration_ms.unwrap_or(1000).max(1);
        let s_ms = start_ms.unwrap_or(0).min(total_ms);
        let e_ms = end_ms.unwrap_or(total_ms).min(total_ms).max(s_ms + 1);
        let span_secs = ((e_ms - s_ms) as f64) / 1000.0;
        let sample_fps = (f64::from(slots) / span_secs).max(0.1);
        let out_file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .map_err(anyhow::Error::from)?;
        let vf = format!("fps={sample_fps:.4},scale=320:-2,tile={cols}x{rows}:margin=4:padding=4");
        let out = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-ss",
                &format!("{:.3}", (s_ms as f64) / 1000.0),
                "-to",
                &format!("{:.3}", (e_ms as f64) / 1000.0),
                "-i",
            ])
            .arg(tmp.path())
            .args(["-frames:v", "1", "-vf", &vf])
            .arg(out_file.path())
            .output()
            .map_err(|e| ApiError::invalid(format!("ffmpeg unavailable: {e}")))?;
        if !out.status.success() {
            return Err(ApiError::invalid(format!(
                "ffmpeg contact_sheet failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        std::fs::read(out_file.path()).map_err(|e| ApiError::Internal(e.into()))
    })
    .await
    .map_err(anyhow::Error::from)??;

    let total = u64::try_from(png_bytes.len()).map_err(anyhow::Error::from)?;
    let len = png_bytes.len();
    Ok(ArtifactChunk {
        id: artifact_id.to_owned(),
        mime: "image/png".to_owned(),
        bytes: total,
        offset: 0,
        length: len,
        data: base64::engine::general_purpose::STANDARD.encode(png_bytes),
    })
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn select_indices(total_frames: u64, fps: f64, strategy: &FrameSampleStrategy) -> Vec<u64> {
    let max_idx = total_frames.saturating_sub(1);
    match strategy {
        FrameSampleStrategy::Coarse { count } => {
            let n = u64::from((*count).clamp(1, 60)).min(total_frames.max(1));
            if n <= 1 {
                return vec![0];
            }
            (0..n)
                .map(|i| (i * max_idx) / (n - 1))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        }
        FrameSampleStrategy::Bisect {
            start_ms,
            end_ms,
            count,
            exclude_frames,
        } => {
            let excluded: BTreeSet<u64> = exclude_frames.iter().copied().collect();
            let s_idx = (((*start_ms as f64) / 1000.0) * fps).floor() as u64;
            let e_idx = (((*end_ms as f64) / 1000.0) * fps).ceil() as u64;
            let s = s_idx.min(max_idx);
            let e = e_idx.min(max_idx).max(s);
            let candidates: Vec<u64> = (s..=e).filter(|i| !excluded.contains(i)).collect();
            let target = (*count).clamp(1, 60) as usize;
            if candidates.len() <= target {
                return candidates;
            }
            (0..target)
                .map(|i| candidates[(i * (candidates.len() - 1)) / (target - 1).max(1)])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        }
        FrameSampleStrategy::Window {
            start_ms,
            end_ms,
            fps: target_fps,
        } => {
            let s_idx = (((*start_ms as f64) / 1000.0) * fps).floor() as u64;
            let e_idx = (((*end_ms as f64) / 1000.0) * fps).ceil() as u64;
            let s = s_idx.min(max_idx);
            let e = e_idx.min(max_idx).max(s);
            let step = (fps / f64::from((*target_fps).clamp(1, 60)))
                .round()
                .max(1.0) as u64;
            (s..=e).step_by(step as usize).take(120).collect()
        }
    }
}

struct ExtractedFrames {
    total_frames: u64,
    fps: f64,
    frames: Vec<(u64, Vec<u8>)>,
}

fn extract_frame_pngs(
    bytes: &[u8],
    mime: &str,
    id: &str,
    strategy: &FrameSampleStrategy,
) -> Result<ExtractedFrames, ApiError> {
    let tmp = write_temp_media(bytes, mime)?;
    let info = probe_video_path(tmp.path(), id, mime)?;
    let fps = info.fps.unwrap_or(60.0).max(1.0);
    let total = info.frame_count.unwrap_or(1).max(1);
    let indices = select_indices(total, fps, strategy);
    if indices.is_empty() {
        return Ok(ExtractedFrames {
            total_frames: total,
            fps,
            frames: Vec::new(),
        });
    }
    let expr = indices
        .iter()
        .map(|n| format!("eq(n\\,{n})"))
        .collect::<Vec<_>>()
        .join("+");
    let dir = tempfile::tempdir().map_err(anyhow::Error::from)?;
    let pattern = dir.path().join("frame-%04d.png");
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(tmp.path())
        .args(["-vf", &format!("select='{expr}'"), "-vsync", "0"])
        .arg(&pattern)
        .output()
        .map_err(|e| ApiError::invalid(format!("ffmpeg unavailable: {e}")))?;
    if !out.status.success() {
        return Err(ApiError::invalid(format!(
            "ffmpeg sample_frames failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let mut files = Vec::new();
    for (pos, frame_idx) in indices.iter().enumerate() {
        let p = dir.path().join(format!("frame-{:04}.png", pos + 1));
        if let Ok(png) = std::fs::read(&p) {
            files.push((*frame_idx, png));
        }
    }
    Ok(ExtractedFrames {
        total_frames: total,
        fps,
        frames: files,
    })
}

fn build_sampled_record(
    cap_id: String,
    project: String,
    parent_capture_id: String,
    members: Vec<Member>,
) -> CaptureRecord {
    CaptureRecord {
        format: RECORD_FORMAT,
        id: cap_id,
        project,
        kind: RecordKind::Derived,
        scenario: ScenarioRef {
            name: "media-sample".to_owned(),
            sha256: None,
        },
        revision: None,
        series: "media-sample@default@default".to_owned(),
        parents: vec![ParentRef {
            id: parent_capture_id,
            kind: ParentKind::Input,
        }],
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
                backend: Some("rupicola-media-inspect".to_owned()),
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
        members,
    }
}

/// Extracts frames according to a progressive [`FrameSampleStrategy`], stores them as a
/// derived capture with `ClipFrame` members, and returns their `artifact_id`s.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub async fn sample_frames(
    state: &Shared,
    principal: &Principal,
    artifact_id: &str,
    strategy: &FrameSampleStrategy,
) -> Result<SampledFrames, ApiError> {
    let (blob, bytes) =
        captures::read_artifact_range(state, principal, artifact_id, 0, MAX_MEDIA_BYTES).await?;
    let id = artifact_id.to_owned();
    let mime = blob.mime.clone();
    let strategy = strategy.clone();
    let ExtractedFrames {
        total_frames,
        fps,
        frames: extracted,
    } = tokio::task::spawn_blocking(move || extract_frame_pngs(&bytes, &mime, &id, &strategy))
        .await
        .map_err(anyhow::Error::from)??;

    if extracted.is_empty() {
        return Ok(SampledFrames {
            source_artifact_id: artifact_id.to_owned(),
            total_frames,
            frames: Vec::new(),
        });
    }

    let project = blob.project_id;
    let mut members = Vec::with_capacity(extracted.len());
    let mut meta = Vec::with_capacity(extracted.len());
    for (frame_idx, png) in extracted {
        let sha = sha256_hex(&png);
        let len = u64::try_from(png.len()).map_err(anyhow::Error::from)?;
        let ts_ms = (((frame_idx as f64) / fps) * 1000.0).round() as u64;
        captures::put_project_blob(state, &project, sha.clone(), png).await?;
        let path = format!("clips/frame-{frame_idx:05}.png");
        members.push(Member {
            path: path.clone(),
            role: MemberRole::ClipFrame,
            mime: "image/png".to_owned(),
            sha256: sha,
            bytes: len,
            owner: format!("frame-{frame_idx:05}"),
            target_ms: Some(ts_ms),
            video_ms: Some(ts_ms),
            surface: None,
            locator: BTreeMap::new(),
        });
        meta.push((frame_idx, ts_ms, path));
    }

    let cap_id = Uuid::now_v7().to_string();
    let record = build_sampled_record(cap_id.clone(), project.clone(), blob.capture_id, members);
    let published = publish::publish(state, principal, &project, &cap_id, &record).await?;
    let cap = match published {
        publish::Published::New(c) | publish::Published::Same(c) => c,
    };
    let by_path: BTreeMap<String, String> = cap
        .clips
        .iter()
        .flat_map(|c| c.frames.iter())
        .map(|f| (f.artifact.path.clone(), f.artifact.id.clone()))
        .chain(
            cap.checkpoints
                .iter()
                .map(|c| (c.artifact.path.clone(), c.artifact.id.clone())),
        )
        .collect();

    let frames = meta
        .into_iter()
        .filter_map(|(frame_index, timestamp_ms, path)| {
            Some(SampledFrame {
                frame_index,
                timestamp_ms,
                artifact_id: by_path.get(&path)?.clone(),
            })
        })
        .collect();

    Ok(SampledFrames {
        source_artifact_id: artifact_id.to_owned(),
        total_frames,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_handles_fractions_and_invalid() {
        assert!((parse_rate("60/1").unwrap() - 60.0).abs() < 1e-6);
        assert!((parse_rate("60000/1001").unwrap() - 59.940_059).abs() < 1e-4);
        assert!(parse_rate("0/0").is_none());
        assert!(parse_rate("invalid").is_none());
    }

    #[test]
    fn progressive_sampling_strategies_never_repeat_excluded_frames() {
        let coarse = select_indices(120, 60.0, &FrameSampleStrategy::Coarse { count: 15 });
        assert_eq!(coarse.len(), 15);
        assert_eq!(*coarse.first().unwrap(), 0);
        assert_eq!(*coarse.last().unwrap(), 119);

        let bisect = select_indices(
            120,
            60.0,
            &FrameSampleStrategy::Bisect {
                start_ms: 0,
                end_ms: 2000,
                count: 15,
                exclude_frames: coarse.clone(),
            },
        );
        assert_eq!(bisect.len(), 15);
        for idx in &bisect {
            assert!(
                !coarse.contains(idx),
                "bisect repeated already-read frame {idx}"
            );
        }

        let window = select_indices(
            120,
            60.0,
            &FrameSampleStrategy::Window {
                start_ms: 500,
                end_ms: 700,
                fps: 60,
            },
        );
        assert_eq!(window.first().copied(), Some(30));
        assert_eq!(window.last().copied(), Some(42));
    }

    #[test]
    fn ffmpeg_probe_and_extract_work_on_synthetic_clip() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mp4_path = dir.path().join("clip.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=0.5:r=30",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&mp4_path)
            .status()
            .unwrap();
        assert!(status.success());

        let info = probe_video_path(&mp4_path, "art-mp4", "video/mp4").unwrap();
        assert_eq!((info.width, info.height), (64, 64));
        assert!(info.frame_count.unwrap_or(0) >= 10);

        let bytes = std::fs::read(&mp4_path).unwrap();
        let extracted = extract_frame_pngs(
            &bytes,
            "video/mp4",
            "art-mp4",
            &FrameSampleStrategy::Coarse { count: 4 },
        )
        .unwrap();
        assert_eq!(extracted.frames.len(), 4);
    }
}
