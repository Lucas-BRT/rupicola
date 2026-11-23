//! What a project may upload: the server's defaults, what an administrator set for the
//! server, and what an owner tightened for the project.

use super::ApiError;
use crate::config::UploadLimits;
use rupicola_api::{LimitSetting, LimitValue, Limits, MAX_MISSING_BATCH};

/// The limits the server starts from, before any setting.
#[derive(Clone, Debug)]
pub struct LimitDefaults(Limits);

impl LimitDefaults {
    /// The defaults from the largest file and body the server accepts, and the upload flags.
    pub fn new(blob_limit: usize, body_limit: usize, uploads: &UploadLimits) -> Self {
        let bytes = |limit: usize| u64::try_from(limit).unwrap_or(u64::MAX);
        Self(
            Limits {
                max_blob_bytes: bytes(blob_limit),
                max_image_bytes: uploads.image_limit,
                max_video_bytes: uploads.video_limit,
                max_artifacts_per_capture: uploads.max_artifacts,
                max_body_bytes: bytes(body_limit),
                max_missing_batch: u32::try_from(MAX_MISSING_BATCH).unwrap_or(u32::MAX),
                upload_timeout_secs: uploads.upload_timeout,
                upload_idle_secs: uploads.upload_idle,
            }
            .clamped(),
        )
    }

    /// The limits the server allows: the defaults, then what an administrator set.
    pub fn server(&self, set: &[(LimitSetting, u64)]) -> Limits {
        let mut limits = self.0.clone();
        for (key, value) in set {
            limits.set(*key, *value);
        }
        limits.clamped()
    }
}

/// The limits that hold for a project: the server's, then what its owner set, which can
/// only tighten.
pub fn effective(server: Limits, set: &[(LimitSetting, u64)]) -> Limits {
    let mut limits = server;
    for (key, value) in set {
        limits.tighten(*key, *value);
    }
    limits.clamped()
}

/// A stored setting that is a limit with a positive whole value. Other settings share the
/// tables, and a value that is not a positive number is not a limit.
pub fn setting(key: &str, value: &str) -> Option<(LimitSetting, u64)> {
    let key = key.parse().ok()?;
    let value = serde_json::from_str::<u64>(value).ok().filter(|v| *v > 0)?;
    Some((key, value))
}

/// The setting a path names.
pub fn key(raw: &str) -> Result<LimitSetting, ApiError> {
    raw.parse().map_err(|_| {
        ApiError::invalid(format!(
            "`{raw}` is not a setting; use one of {}",
            LimitSetting::NAMES.join(", ")
        ))
    })
}

pub fn positive(value: LimitValue) -> Result<u64, ApiError> {
    if value.value == 0 {
        Err(ApiError::invalid("a limit must be greater than zero"))
    } else {
        Ok(value.value)
    }
}

/// Fails unless a project's `value` for `key` stays within what the server `allowed`.
pub fn within_server(key: LimitSetting, value: u64, allowed: u64) -> Result<(), ApiError> {
    if value > allowed {
        Err(ApiError::invalid(format!(
            "the server allows at most {allowed} for {key}; a project can only tighten it"
        )))
    } else {
        Ok(())
    }
}
