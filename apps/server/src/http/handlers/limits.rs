//! `/projects/{id}/limits`: what the caller may upload to a project.

use crate::{
    domain::{ApiError, Principal},
    services::limits,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
};
use rupicola_api::Limits;

/// `GET /v1/projects/{id}/limits`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Limits>, ApiError> {
    limits::of_project(&state, &principal, &project)
        .await
        .map(Json)
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
            ceiling.min(crate::config::DEFAULT_IMAGE_LIMIT)
        );
        assert_eq!(
            limits.max_video_bytes,
            ceiling.min(crate::config::DEFAULT_VIDEO_LIMIT)
        );
        assert_eq!(
            limits.max_artifacts_per_capture,
            crate::config::DEFAULT_MAX_ARTIFACTS
        );
        assert_eq!(
            limits.upload_timeout_secs,
            crate::config::DEFAULT_UPLOAD_TIMEOUT_SECS
        );
        assert_eq!(
            limits.upload_idle_secs,
            crate::config::DEFAULT_UPLOAD_IDLE_SECS
        );
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
            crate::config::DEFAULT_UPLOAD_TIMEOUT_SECS,
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
