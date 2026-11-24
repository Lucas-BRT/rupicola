//! Settings the server reads at start from flags or environment variables, other than
//! the request hardening in [`crate::http::security::Security`].

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

/// Capture runner settings from command-line flags or environment variables.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct CaptureConfig {
    /// Allow server-executed captures to target loopback, private, or link-local addresses.
    #[arg(long, env = "RUPICOLA_ALLOW_LOCAL_CAPTURES")]
    pub allow_local_captures: bool,
}
