//! Editing the upload limits: an administrator sets them for the server, and a project's owner
//! tightens them for the project. Setting nothing leaves the level above in charge.

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    limits::LimitDefaults,
    projects::{access, audit, person},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
};
use rupicola_api::{LimitSetting, LimitSettings, LimitValue, Role};

/// The routes of the limits and their settings.
pub fn routes() -> axum::Router<Shared> {
    use axum::routing;
    axum::Router::new()
        .route("/projects/{id}/limits", routing::get(crate::limits::get))
        .route("/settings", routing::get(server))
        .route(
            "/settings/{key}",
            routing::put(set_server).delete(reset_server),
        )
        .route("/projects/{id}/settings", routing::get(project))
        .route(
            "/projects/{id}/settings/{key}",
            routing::put(set_project).delete(reset_project),
        )
}

/// The setting a path names.
fn key(raw: &str) -> Result<LimitSetting, ApiError> {
    raw.parse().map_err(|_| {
        ApiError::invalid(format!(
            "`{raw}` is not a setting; use one of {}",
            LimitSetting::NAMES.join(", ")
        ))
    })
}

fn positive(value: LimitValue) -> Result<u64, ApiError> {
    if value.value == 0 {
        Err(ApiError::invalid("a limit must be greater than zero"))
    } else {
        Ok(value.value)
    }
}

/// Only an administrator changes the server; the name they act under.
fn administrator(principal: &Principal) -> Result<&str, ApiError> {
    let handle = person(principal)?;
    if principal.admin {
        Ok(handle)
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn server_view(state: &Shared) -> Result<Json<LimitSettings>, ApiError> {
    Ok(Json(LimitSettings {
        set: LimitDefaults::stored(state, None)
            .await?
            .into_iter()
            .collect(),
        effective: state.limits.server(state).await?,
    }))
}

async fn project_view(state: &Shared, project: &str) -> Result<Json<LimitSettings>, ApiError> {
    Ok(Json(LimitSettings {
        set: LimitDefaults::stored(state, Some(project))
            .await?
            .into_iter()
            .collect(),
        effective: state.limits.effective(state, project).await?,
    }))
}

/// `GET /v1/settings`: the limits an administrator set for the server, and what they make.
pub async fn server(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<LimitSettings>, ApiError> {
    administrator(&principal)?;
    server_view(&state).await
}

/// `PUT /v1/settings/{key}`: sets a limit for the whole server. It may be higher or lower than
/// the value the server started with.
pub async fn set_server(
    State(state): State<Shared>,
    principal: Principal,
    Path(raw): Path<String>,
    Json(value): Json<LimitValue>,
) -> Result<Json<LimitSettings>, ApiError> {
    let actor = administrator(&principal)?.to_owned();
    let key = key(&raw)?;
    let value = positive(value)?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO server_settings (key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, 0, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, generation = generation + 1,
           updated_by = excluded.updated_by, updated_at = excluded.updated_at",
    )
    .bind(key.as_str())
    .bind(value.to_string())
    .bind(&actor)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        None,
        &actor,
        &format!("Set the server's {key} to {value}"),
    )
    .await?;
    tx.commit().await?;
    server_view(&state).await
}

/// `DELETE /v1/settings/{key}`: the limit goes back to the value the server started with.
pub async fn reset_server(
    State(state): State<Shared>,
    principal: Principal,
    Path(raw): Path<String>,
) -> Result<Json<LimitSettings>, ApiError> {
    let actor = administrator(&principal)?.to_owned();
    let key = key(&raw)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM server_settings WHERE key = ?1")
        .bind(key.as_str())
        .execute(&mut *tx)
        .await?;
    audit(&mut *tx, None, &actor, &format!("Reset the server's {key}")).await?;
    tx.commit().await?;
    server_view(&state).await
}

/// `GET /v1/projects/{id}/settings`: the limits the owner set for the project, and what holds.
pub async fn project(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<LimitSettings>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    project_view(&state, &project).await
}

/// `PUT /v1/projects/{id}/settings/{key}`: tightens a limit for the project. A value above what
/// the server allows is refused rather than quietly ignored.
pub async fn set_project(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, raw)): Path<(String, String)>,
    Json(value): Json<LimitValue>,
) -> Result<Json<LimitSettings>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?.to_owned();
    let key = key(&raw)?;
    let value = positive(value)?;
    let allowed = state.limits.server(&state).await?.get(key);
    if value > allowed {
        return Err(ApiError::invalid(format!(
            "the server allows at most {allowed} for {key}; a project can only tighten it"
        )));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO project_settings (project_id, key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value,
           generation = generation + 1, updated_by = excluded.updated_by,
           updated_at = excluded.updated_at",
    )
    .bind(&project)
    .bind(key.as_str())
    .bind(value.to_string())
    .bind(&actor)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Set {key} to {value}"),
    )
    .await?;
    tx.commit().await?;
    project_view(&state, &project).await
}

/// `DELETE /v1/projects/{id}/settings/{key}`: the project follows the server for this limit
/// again.
pub async fn reset_project(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, raw)): Path<(String, String)>,
) -> Result<Json<LimitSettings>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?.to_owned();
    let key = key(&raw)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM project_settings WHERE project_id = ?1 AND key = ?2")
        .bind(&project)
        .bind(key.as_str())
        .execute(&mut *tx)
        .await?;
    audit(&mut *tx, Some(&project), &actor, &format!("Reset {key}")).await?;
    tx.commit().await?;
    project_view(&state, &project).await
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
            super::super::limits::DEFAULT_MAX_ARTIFACTS
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
}
