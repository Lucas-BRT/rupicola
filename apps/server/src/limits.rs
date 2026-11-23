//! What a project may upload: the server's defaults, what an administrator set for the
//! server, and what an owner tightened for the project.

use crate::{
    domain::Access, domain::ApiError, domain::Principal, http::security::Security,
    services::access::access, state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
};
use rupicola_api::{LimitSetting, Limits, MAX_MISSING_BATCH, Role};

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
/// kind is `RUPICOLA_BLOB_LIMIT`, in [`Security`].
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

/// The limits the server starts from, before any setting.
#[derive(Clone, Debug)]
pub struct LimitDefaults(Limits);

impl LimitDefaults {
    pub fn new(security: &Security, uploads: &UploadLimits) -> Self {
        let bytes = |limit: usize| u64::try_from(limit).unwrap_or(u64::MAX);
        Self(
            Limits {
                max_blob_bytes: bytes(security.blob_limit),
                max_image_bytes: uploads.image_limit,
                max_video_bytes: uploads.video_limit,
                max_artifacts_per_capture: uploads.max_artifacts,
                max_body_bytes: bytes(security.body_limit),
                max_missing_batch: u32::try_from(MAX_MISSING_BATCH).unwrap_or(u32::MAX),
                upload_timeout_secs: uploads.upload_timeout,
                upload_idle_secs: uploads.upload_idle,
            }
            .clamped(),
        )
    }

    /// The limits the server allows: the defaults, then what an administrator set.
    ///
    /// # Errors
    ///
    /// When the settings cannot be read.
    pub async fn server(&self, state: &crate::state::AppState) -> Result<Limits, ApiError> {
        let mut limits = self.0.clone();
        for (key, value) in Self::stored(state, None).await? {
            limits.set(key, value);
        }
        Ok(limits.clamped())
    }

    /// The limits that hold for `project`: the server's, then what its owner set, which can
    /// only tighten.
    ///
    /// # Errors
    ///
    /// When the settings cannot be read.
    pub async fn effective(
        &self,
        state: &crate::state::AppState,
        project: &str,
    ) -> Result<Limits, ApiError> {
        let mut limits = self.server(state).await?;
        for (key, value) in Self::stored(state, Some(project)).await? {
            limits.tighten(key, value);
        }
        Ok(limits.clamped())
    }

    /// The limits stored for the server, or for `project`.
    ///
    /// # Errors
    ///
    /// When the settings cannot be read.
    pub async fn stored(
        state: &crate::state::AppState,
        project: Option<&str>,
    ) -> Result<Vec<(LimitSetting, u64)>, ApiError> {
        let rows: Vec<(String, String)> = match project {
            None => {
                sqlx::query_as("SELECT key, value FROM server_settings")
                    .fetch_all(&state.db)
                    .await?
            }
            Some(project) => {
                sqlx::query_as("SELECT key, value FROM project_settings WHERE project_id = ?1")
                    .bind(project)
                    .fetch_all(&state.db)
                    .await?
            }
        };
        Ok(rows
            .iter()
            .filter_map(|(key, value)| Self::setting(key, value))
            .collect())
    }

    /// A stored setting that is a limit with a positive whole value. Other settings share the
    /// tables, and a value that is not a positive number is not a limit.
    fn setting(key: &str, value: &str) -> Option<(LimitSetting, u64)> {
        let key = key.parse().ok()?;
        let value = serde_json::from_str::<u64>(value).ok().filter(|v| *v > 0)?;
        Some((key, value))
    }
}

/// `GET /v1/projects/{id}/limits`: what the caller may upload to the project.
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Limits>, ApiError> {
    // A capture device uploads to its project, so it reads the limits too.
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reader)?,
    }
    Ok(Json(state.limits.effective(&state, &project).await?))
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::Limits;

    const LIMITS: &str = "/v1/projects/demo/limits";
    const DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    async fn demo() -> (u16, Shared, String) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        let ana = sign_in(port, "ana").await;
        let created = call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        (port, state, ana)
    }

    async fn limits(port: u16, token: &str) -> Limits {
        let response = call(port, "GET", LIMITS, Some(token), None).await;
        assert_eq!(status(&response), StatusCode::OK, "{response}");
        json(&response)
    }

    async fn store(state: &crate::state::AppState, sql: &str) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&state.db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_defaults_come_from_the_flags_and_no_kind_exceeds_the_ceiling() {
        let (port, _, ana) = demo().await;
        let limits = limits(port, &ana).await;
        let ceiling = u64::try_from(settings().blob_limit).unwrap();
        assert_eq!(limits.max_blob_bytes, ceiling);
        assert_eq!(
            limits.max_image_bytes,
            ceiling.min(super::DEFAULT_IMAGE_LIMIT)
        );
        assert_eq!(
            limits.max_video_bytes,
            ceiling.min(super::DEFAULT_VIDEO_LIMIT)
        );
        assert_eq!(
            limits.max_artifacts_per_capture,
            super::DEFAULT_MAX_ARTIFACTS
        );
        assert_eq!(
            limits.upload_timeout_secs,
            super::DEFAULT_UPLOAD_TIMEOUT_SECS
        );
        assert_eq!(limits.upload_idle_secs, super::DEFAULT_UPLOAD_IDLE_SECS);
        assert_eq!(limits.max_missing_batch, 1000);
    }

    #[tokio::test]
    async fn the_server_sets_a_limit_and_a_project_can_only_tighten_it() {
        let (port, state, ana) = demo().await;
        store(
            &state,
            "INSERT INTO server_settings (key, value, updated_at) VALUES
               ('max_artifacts_per_capture', '10', 0),
               ('upload_idle_secs', '5', 0),
               ('cited_capture_deletion', '\"confirm\"', 0),
               ('upload_timeout_secs', '\"soon\"', 0);
             INSERT INTO project_settings (project_id, key, value, updated_at) VALUES
               ('demo', 'max_artifacts_per_capture', '3', 0),
               ('demo', 'upload_idle_secs', '60', 0),
               ('demo', 'upload_timeout_secs', '0', 0);",
        )
        .await;
        let limits = limits(port, &ana).await;
        assert_eq!(
            limits.max_artifacts_per_capture, 3,
            "the project tightens the server"
        );
        assert_eq!(
            limits.upload_idle_secs, 5,
            "the project cannot loosen the server"
        );
        assert_eq!(
            limits.upload_timeout_secs,
            super::DEFAULT_UPLOAD_TIMEOUT_SECS,
            "a value that is not a positive number is not a limit"
        );
    }

    #[tokio::test]
    async fn a_capture_with_more_files_than_the_project_allows_is_refused() {
        let (port, state, ana) = demo().await;
        store(
            &state,
            "INSERT INTO project_settings (project_id, key, value, updated_at)
               VALUES ('demo', 'max_artifacts_per_capture', '1', 0)",
        )
        .await;
        let form = format!(
            r#"{{"scenario":"Home","status":"complete","checkpoints":[
                {{"id":"a","path":"a.png","sha256":"{DIGEST}"}},
                {{"id":"b","path":"b.png","sha256":"{DIGEST}"}}]}}"#
        );
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&form),
        )
        .await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
        assert!(body(&response).contains("at most 1 files"), "{response}");
    }

    #[tokio::test]
    async fn only_people_with_access_read_a_projects_limits() {
        let (port, state, _) = demo().await;
        seed_account(&state, "bruno", false).await;
        let bruno = sign_in(port, "bruno").await;
        let response = call(port, "GET", LIMITS, Some(&bruno), None).await;
        assert_eq!(status(&response), StatusCode::NOT_FOUND, "{response}");
        let anonymous = call(port, "GET", LIMITS, None, None).await;
        assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED);
    }
}
