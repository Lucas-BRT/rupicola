//! Receiving a file as it streams in, under the project's limits: the largest file of its
//! kind (told by its first bytes), the longest the whole upload may take and the longest it
//! may go without a byte. Nothing is kept unless the content matches the digest.

use crate::{
    domain::{
        ApiError, Principal,
        blob::{BlobKind, MAGIC_LEN},
    },
    repositories::blobs,
    services::{access::require_publisher, limits},
    state::AppState,
    storage::blobs::Upload,
};
use rupicola_api::{Limits, Role};
use std::time::Duration;

/// A file being received into a project.
pub struct Receiving {
    project: String,
    sha256: String,
    limits: Limits,
    upload: Upload,
    head: Vec<u8>,
    limit: Option<u64>,
}

impl Receiving {
    /// Starts receiving the file `sha256` into `project`, for a caller who may publish there.
    pub async fn begin(
        state: &AppState,
        principal: &Principal,
        project: &str,
        sha256: &str,
    ) -> Result<Self, ApiError> {
        require_publisher(state, principal, project, Role::Reviewer).await?;
        let limits = limits::effective(state, project).await?;
        Ok(Self {
            project: project.to_owned(),
            sha256: sha256.to_owned(),
            upload: state.blobs.begin(sha256)?,
            limits,
            head: Vec::with_capacity(MAGIC_LEN),
            limit: None,
        })
    }

    /// How long the whole upload may take.
    pub const fn timeout(&self) -> Duration {
        Duration::from_secs(self.limits.upload_timeout_secs)
    }

    /// How long the upload may go without a byte.
    pub const fn idle(&self) -> Duration {
        Duration::from_secs(self.limits.upload_idle_secs)
    }

    /// Takes the next piece; refused once the file outgrows what its kind may be.
    pub async fn take(&mut self, chunk: &[u8]) -> Result<(), ApiError> {
        if self.limit.is_none() {
            let take = (MAGIC_LEN - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            if self.head.len() == MAGIC_LEN {
                self.limit = Some(BlobKind::of(&self.head).limit(&self.limits));
            }
        }
        let allowed = self.limit.unwrap_or(self.limits.max_blob_bytes);
        let after =
            self.upload.bytes() + u64::try_from(chunk.len()).map_err(anyhow::Error::from)?;
        if after > allowed {
            return Err(ApiError::PayloadTooLarge);
        }
        self.upload.write(chunk).await
    }

    /// Keeps the file and records that the project holds it. Returns whether it was new.
    pub async fn finish(self, state: &AppState) -> Result<bool, ApiError> {
        // A file shorter than the magic bytes is checked against its kind at the end.
        if self.upload.bytes() > BlobKind::of(&self.head).limit(&self.limits) {
            return Err(ApiError::PayloadTooLarge);
        }
        let bytes = self.upload.bytes();
        let created = self.upload.finish(&state.blobs).await?;
        let mut connection = state.db.acquire().await?;
        blobs::record(&mut connection, &self.project, &self.sha256, bytes).await?;
        Ok(created)
    }
}
