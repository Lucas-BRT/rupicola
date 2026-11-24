//! Settings the server reads at start from flags or environment variables, other than
//! the request hardening in [`crate::http::security::Security`].

use crate::domain::{limits::LimitDefaults, removal::CitedDeletion};
use rupicola_api::{Limits, MAX_MISSING_BATCH};

const MIB: u64 = 1024 * 1024;
/// Largest image, unless an administrator sets another.
pub const DEFAULT_IMAGE_LIMIT: u64 = 32 * MIB;
/// Largest video, unless an administrator sets another.
pub const DEFAULT_VIDEO_LIMIT: u64 = 512 * MIB;
/// Files in one capture, unless an administrator sets another.
pub const DEFAULT_MAX_ARTIFACTS: u32 = 1024;
/// Seconds one file upload may take in all.
pub const DEFAULT_UPLOAD_TIMEOUT_SECS: u64 = 600;
/// Seconds an upload may go without a byte before it is cut.
pub const DEFAULT_UPLOAD_IDLE_SECS: u64 = 30;

/// Upload limits from command-line flags or environment variables. The largest file of any
/// kind is `RUPICOLA_BLOB_LIMIT`, in [`crate::http::security::Security`].
#[derive(Clone, Debug, clap::Args)]
pub struct UploadLimits {
    /// Largest image (`png`, `gif`), in bytes; never above the blob limit.
    #[arg(long, env = "RUPICOLA_IMAGE_LIMIT", default_value_t = DEFAULT_IMAGE_LIMIT)]
    pub image_limit: u64,
    /// Largest video (`mp4`, `webm`), in bytes; never above the blob limit.
    #[arg(long, env = "RUPICOLA_VIDEO_LIMIT", default_value_t = DEFAULT_VIDEO_LIMIT)]
    pub video_limit: u64,
    /// Files in one capture.
    #[arg(long, env = "RUPICOLA_MAX_ARTIFACTS", default_value_t = DEFAULT_MAX_ARTIFACTS)]
    pub max_artifacts: u32,
    /// Seconds one file upload may take in all.
    #[arg(long, env = "RUPICOLA_UPLOAD_TIMEOUT", default_value_t = DEFAULT_UPLOAD_TIMEOUT_SECS)]
    pub upload_timeout: u64,
    /// Seconds an upload may go without a byte before it is cut.
    #[arg(long, env = "RUPICOLA_UPLOAD_IDLE", default_value_t = DEFAULT_UPLOAD_IDLE_SECS)]
    pub upload_idle: u64,
}

impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            image_limit: DEFAULT_IMAGE_LIMIT,
            video_limit: DEFAULT_VIDEO_LIMIT,
            max_artifacts: DEFAULT_MAX_ARTIFACTS,
            upload_timeout: DEFAULT_UPLOAD_TIMEOUT_SECS,
            upload_idle: DEFAULT_UPLOAD_IDLE_SECS,
        }
    }
}

impl UploadLimits {
    /// Builds [`LimitDefaults`] from the server's blob/body ceilings and upload flags.
    #[must_use]
    pub fn to_defaults(&self, blob_limit: usize, body_limit: usize) -> LimitDefaults {
        let bytes = |limit: usize| u64::try_from(limit).unwrap_or(u64::MAX);
        LimitDefaults::new(Limits {
            max_blob_bytes: bytes(blob_limit),
            max_image_bytes: self.image_limit,
            max_video_bytes: self.video_limit,
            max_artifacts_per_capture: self.max_artifacts,
            max_body_bytes: bytes(body_limit),
            max_missing_batch: u32::try_from(MAX_MISSING_BATCH).unwrap_or(u32::MAX),
            upload_timeout_secs: self.upload_timeout,
            upload_idle_secs: self.upload_idle,
        })
    }
}

/// CLI/env wrapper for [`CitedDeletion`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum CitedDeletionArg {
    /// Each project's policy decides.
    #[default]
    Allow,
    /// No cited capture is ever removed.
    Never,
}

impl From<CitedDeletionArg> for CitedDeletion {
    fn from(arg: CitedDeletionArg) -> Self {
        match arg {
            CitedDeletionArg::Allow => Self::Allow,
            CitedDeletionArg::Never => Self::Never,
        }
    }
}

/// Capture runner settings from command-line flags or environment variables.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct CaptureConfig {
    /// Allow server-executed captures to target loopback, private, or link-local addresses.
    #[arg(long, env = "RUPICOLA_ALLOW_LOCAL_CAPTURES")]
    pub allow_local_captures: bool,
}
