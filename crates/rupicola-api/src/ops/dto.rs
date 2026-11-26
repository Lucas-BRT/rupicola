//! Shared request and response DTOs for operation traits, including planned
//! capture-execution and progressive token-efficient video/media inspection.

use serde::{Deserialize, Serialize};

/// Request payload for publishing a capture directly (inline base64 or pre-uploaded blobs).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PublishCaptureRequest {
    pub project: String,
    pub scenario: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    pub members: Vec<PublishMember>,
}

/// One media file in a [`PublishCaptureRequest`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PublishMember {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// One base64-encoded slice of a stored artifact returned by `artifact_read`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ArtifactChunk {
    pub id: String,
    pub mime: String,
    pub bytes: u64,
    pub offset: u64,
    pub length: usize,
    pub data: String,
}

/// Raw binary slice returned by `BlobTransferOps::artifact_bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactBytes {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_range: Option<String>,
    pub bytes: Vec<u8>,
}

// ── Planned Capture Execution DTOs (rupicola-capture orchestration) ───────────

/// Request to execute a stored or inline scenario via `rupicola-capture`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureRunRequest {
    pub project: String,
    pub scenario: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

/// Request to record an interaction video clip via `rupicola-capture`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RecordClipRequest {
    pub project: String,
    pub url: String,
    pub fps: u32,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

/// Request to take a single checkpoint screenshot (+ optional UI snapshot).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SnapshotCheckpointRequest {
    pub project: String,
    pub url: String,
    pub label: String,
    #[serde(default)]
    pub include_ui_snapshot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

// ── Planned Token-Efficient Media / Video Inspection DTOs ─────────────────────

/// Lightweight video/image metadata returned before decoding any frames into context.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MediaProbe {
    pub artifact_id: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub fps: Option<f64>,
    pub frame_count: Option<u64>,
    pub codec: Option<String>,
}

/// Progressive frame-sampling strategy to inspect high-FPS clips without wasting tokens.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum FrameSampleStrategy {
    /// Step 1: evenly spaced coarse overview (e.g. 15 frames across the clip).
    Coarse { count: u32 },
    /// Step 2+: fill intermediate frames inside `[start_ms, end_ms]` excluding already-read indices.
    Bisect {
        start_ms: u64,
        end_ms: u64,
        count: u32,
        #[serde(default)]
        exclude_frames: Vec<u64>,
    },
    /// Final zoom: extract every frame at `fps` (e.g. 60) strictly inside the suspect window.
    Window {
        start_ms: u64,
        end_ms: u64,
        fps: u32,
    },
}

/// One extracted frame reference with its timestamp and frame index.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SampledFrame {
    pub frame_index: u64,
    pub timestamp_ms: u64,
    pub artifact_id: String,
}

/// Result of a progressive frame sampling pass.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SampledFrames {
    pub source_artifact_id: String,
    pub total_frames: u64,
    pub frames: Vec<SampledFrame>,
}

/// Request to render a single numbered contact sheet for 1-image visual triage.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ContactSheetRequest {
    pub cols: u32,
    pub rows: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

/// One frame-to-frame motion or flash spike detected numerically without opening PNGs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MotionSpike {
    pub frame_index: u64,
    pub timestamp_ms: u64,
    pub delta_ratio: f64,
    pub luma_delta: f64,
}

/// Numeric motion/flicker report over a video clip.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MotionDiffReport {
    pub artifact_id: String,
    pub threshold: f64,
    pub spikes: Vec<MotionSpike>,
}

/// Bounding box in image pixel coordinates.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Pixel-diff visualization mode.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualDiffMode {
    Highlight,
    Heatmap,
    SideBySide,
}

/// Result of comparing two image artifacts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VisualDiffReport {
    pub before_artifact_id: String,
    pub after_artifact_id: String,
    pub diff_artifact_id: String,
    pub changed_ratio: f64,
    pub changed_regions: Vec<CropRect>,
}
