//! `/settings` and `/projects/{id}/settings`: the upload limits an administrator sets for
//! the server and an owner tightens for a project.

use crate::{
    domain::{ApiError, Principal},
    services::settings,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
};
use rupicola_api::{LimitSettings, LimitValue, RecaptureSettings, RecaptureSettingsPatch};

/// `GET /v1/settings`
pub async fn server(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::server(&state, &principal).await.map(Json)
}

/// `PUT /v1/settings/{key}`
pub async fn set_server(
    State(state): State<Shared>,
    principal: Principal,
    Path(raw): Path<String>,
    Json(value): Json<LimitValue>,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::set_server(&state, &principal, &raw, value)
        .await
        .map(Json)
}

/// `DELETE /v1/settings/{key}`
pub async fn reset_server(
    State(state): State<Shared>,
    principal: Principal,
    Path(raw): Path<String>,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::reset_server(&state, &principal, &raw)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/settings`
pub async fn project(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::project(&state, &principal, &project)
        .await
        .map(Json)
}

/// `PUT /v1/projects/{id}/settings/{key}`
pub async fn set_project(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, raw)): Path<(String, String)>,
    Json(value): Json<LimitValue>,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::set_project(&state, &principal, &project, &raw, value)
        .await
        .map(Json)
}

/// `DELETE /v1/projects/{id}/settings/{key}`
pub async fn reset_project(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, raw)): Path<(String, String)>,
) -> Result<Json<LimitSettings>, ApiError> {
    settings::reset_project(&state, &principal, &project, &raw)
        .await
        .map(Json)
}

/// `GET /v1/settings/capture`
pub async fn capture_server(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<RecaptureSettings>, ApiError> {
    settings::capture_server(&state, &principal).await.map(Json)
}

/// `PATCH /v1/settings/capture`
pub async fn update_capture_server(
    State(state): State<Shared>,
    principal: Principal,
    Json(patch): Json<RecaptureSettingsPatch>,
) -> Result<Json<RecaptureSettings>, ApiError> {
    settings::update_capture_server(&state, &principal, &patch)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/capture-settings`
pub async fn capture_project(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<RecaptureSettings>, ApiError> {
    settings::capture_project(&state, &principal, &project)
        .await
        .map(Json)
}

/// `PATCH /v1/projects/{id}/capture-settings`
pub async fn update_capture_project(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(patch): Json<RecaptureSettingsPatch>,
) -> Result<Json<RecaptureSettings>, ApiError> {
    settings::update_capture_project(&state, &principal, &project, &patch)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::{AuditEvent, LimitSetting, LimitSettings, Limits};

    /// Root administers the server; Ana owns `demo`, where Bruno reads.
    async fn demo() -> (u16, Shared, [String; 3]) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "root", true).await;
        for handle in ["ana", "bruno"] {
            seed_account(&state, handle, false).await;
        }
        let [root, ana, bruno] = [
            sign_in(port, "root").await,
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        (port, state, [root, ana, bruno])
    }

    async fn put(port: u16, token: &str, path: &str, value: u64) -> String {
        call(
            port,
            "PUT",
            path,
            Some(token),
            Some(&format!(r#"{{"value":{value}}}"#)),
        )
        .await
    }

    async fn limits(port: u16, token: &str) -> Limits {
        json(&call(port, "GET", "/v1/projects/demo/limits", Some(token), None).await)
    }

    #[tokio::test]
    async fn an_administrator_sets_a_server_limit_and_resets_it() {
        let (port, state, [root, ana, _]) = demo().await;
        let set = put(port, &root, "/v1/settings/max_artifacts_per_capture", 4000).await;
        assert_eq!(status(&set), StatusCode::OK, "{set}");
        let view: LimitSettings = json(&set);
        assert_eq!(
            view.set.get(&LimitSetting::MaxArtifactsPerCapture),
            Some(&4000)
        );
        assert_eq!(
            view.effective.max_artifacts_per_capture, 4000,
            "above the start value too"
        );
        assert_eq!(limits(port, &ana).await.max_artifacts_per_capture, 4000);
        let reset = call(
            port,
            "DELETE",
            "/v1/settings/max_artifacts_per_capture",
            Some(&root),
            None,
        )
        .await;
        let view: LimitSettings = json(&reset);
        assert!(view.set.is_empty());
        assert_eq!(
            view.effective.max_artifacts_per_capture,
            crate::config::DEFAULT_MAX_ARTIFACTS
        );
        let logged: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE project_id IS NULL")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(logged, 2, "both changes are in the audit log");
    }

    #[tokio::test]
    async fn only_an_administrator_touches_the_server_and_only_with_a_known_positive_limit() {
        let (port, _, [root, ana, _]) = demo().await;
        let owner = put(port, &ana, "/v1/settings/max_blob_bytes", 1).await;
        assert_eq!(status(&owner), StatusCode::FORBIDDEN);
        let read = call(port, "GET", "/v1/settings", Some(&ana), None).await;
        assert_eq!(status(&read), StatusCode::FORBIDDEN);
        let unknown = put(port, &root, "/v1/settings/speed_of_light", 1).await;
        assert_eq!(status(&unknown), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            body(&unknown).contains("max_blob_bytes"),
            "the error lists the settings"
        );
        let zero = put(port, &root, "/v1/settings/max_blob_bytes", 0).await;
        assert_eq!(status(&zero), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn an_owner_tightens_the_project_but_cannot_go_past_the_server() {
        let (port, _, [root, ana, bruno]) = demo().await;
        put(port, &root, "/v1/settings/max_artifacts_per_capture", 100).await;
        let tight = put(
            port,
            &ana,
            "/v1/projects/demo/settings/max_artifacts_per_capture",
            5,
        )
        .await;
        assert_eq!(status(&tight), StatusCode::OK, "{tight}");
        assert_eq!(limits(port, &bruno).await.max_artifacts_per_capture, 5);
        let loose = put(
            port,
            &ana,
            "/v1/projects/demo/settings/max_artifacts_per_capture",
            101,
        )
        .await;
        assert_eq!(status(&loose), StatusCode::UNPROCESSABLE_ENTITY, "{loose}");
        assert!(body(&loose).contains("at most 100"), "{loose}");
        let reader = put(
            port,
            &bruno,
            "/v1/projects/demo/settings/max_artifacts_per_capture",
            3,
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
        let view: LimitSettings = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/settings",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(
            view.set.get(&LimitSetting::MaxArtifactsPerCapture),
            Some(&5)
        );
        call(
            port,
            "DELETE",
            "/v1/projects/demo/settings/max_artifacts_per_capture",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(limits(port, &bruno).await.max_artifacts_per_capture, 100);
        let events: Vec<AuditEvent> =
            json(&call(port, "GET", "/v1/projects/demo/audit", Some(&ana), None).await);
        assert!(
            events
                .iter()
                .any(|e| e.event == "Set max_artifacts_per_capture to 5")
        );
        assert!(
            events
                .iter()
                .any(|e| e.event == "Reset max_artifacts_per_capture")
        );
    }

    #[tokio::test]
    async fn capture_settings_round_trip_with_base_ref() {
        let (port, _state, [_root, ana, bruno]) = demo().await;
        let updated = call(
            port,
            "PATCH",
            "/v1/projects/demo/capture-settings",
            Some(&ana),
            Some(r#"{"auto_recapture":false,"base_ref":"main"}"#),
        )
        .await;
        assert_eq!(status(&updated), StatusCode::OK);
        let parsed: rupicola_api::RecaptureSettings = json(&updated);
        assert!(!parsed.auto_recapture);
        assert_eq!(parsed.base_ref, "main");

        let read_back: rupicola_api::RecaptureSettings = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/capture-settings",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert!(!read_back.auto_recapture);
        assert_eq!(read_back.base_ref, "main");
    }
}
