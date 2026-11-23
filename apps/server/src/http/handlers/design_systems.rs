//! `/projects/{id}/design-systems` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::design_systems,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{DocumentEdit, SystemActionForm, SystemList, SystemVersion};

/// `GET /v1/projects/{id}/design-systems`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<SystemList>, ApiError> {
    design_systems::list(&state, &principal, &project)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/design-systems/{version}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
) -> Result<Json<SystemVersion>, ApiError> {
    design_systems::get(&state, &principal, &project, &id)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/design-systems`: a new draft, starting from the adopted version.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<(StatusCode, Json<SystemVersion>), ApiError> {
    design_systems::create(&state, &principal, &project)
        .await
        .map(|version| (StatusCode::CREATED, Json(version)))
}

/// `PUT /v1/projects/{id}/design-systems/{version}/document`
pub async fn edit(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(form): Json<DocumentEdit>,
) -> Result<Json<SystemVersion>, ApiError> {
    design_systems::edit(&state, &principal, &project, &id, form)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/design-systems/{version}/actions`
pub async fn act(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(form): Json<SystemActionForm>,
) -> Result<Json<SystemVersion>, ApiError> {
    design_systems::act(&state, &principal, &project, &id, &form)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::design_system::{Report, fingerprint},
        testkit::*,
    };
    use axum::http::StatusCode;
    use rupicola_api::{Document, DocumentEdit, SystemList, SystemPhase, SystemVersion};

    const BASE: &str = "/v1/projects/demo/design-systems";

    async fn setup() -> (u16, crate::state::Shared, [String; 3]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let tokens = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&tokens[0]),
            Some(r#"{"id":"demo","label":"demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&tokens[0]),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        (port, state, tokens)
    }

    async fn act_on(port: u16, token: &str, id: &str, body: &str) -> String {
        call(
            port,
            "POST",
            &format!("{BASE}/{id}/actions"),
            Some(token),
            Some(body),
        )
        .await
    }

    /// Stores a passing validation.
    async fn pass(state: &crate::state::Shared, version: &SystemVersion, baseline: Option<&str>) {
        let report = Report {
            fingerprint: fingerprint(&version.document),
            baseline: baseline.map(str::to_owned),
            outcome: rupicola_api::Outcome::Passed,
            browser: Some("test".into()),
            checks: vec![],
            shots: vec![],
            error: None,
        };
        sqlx::query(
            "UPDATE design_versions SET report = ?1, generation = generation + 1 WHERE id = ?2",
        )
        .bind(serde_json::to_string(&report).unwrap())
        .bind(&version.id)
        .execute(&state.db)
        .await
        .unwrap();
    }

    async fn fetch(port: u16, token: &str, id: &str) -> SystemVersion {
        json(&call(port, "GET", &format!("{BASE}/{id}"), Some(token), None).await)
    }

    #[tokio::test]
    async fn only_an_owner_writes_and_members_read() {
        let (port, _state, [ana, bruno, carla]) = setup().await;
        let created = call(port, "POST", BASE, Some(&ana), None).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let draft: SystemVersion = json(&created);
        assert_eq!(draft.phase, SystemPhase::Draft);
        assert_eq!(draft.author, "ana");
        assert!(!draft.ready);

        let listed: SystemList = json(&call(port, "GET", BASE, Some(&bruno), None).await);
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.active, None);
        assert_eq!(fetch(port, &bruno, &draft.id).await.id, draft.id);
        assert_eq!(
            status(&call(port, "POST", BASE, Some(&bruno), None).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&call(port, "GET", BASE, Some(&carla), None).await),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(&call(port, "GET", &format!("{BASE}/missing"), Some(&ana), None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn an_edit_needs_a_valid_document_and_the_latest_generation() {
        let (port, _state, [ana, ..]) = setup().await;
        let draft: SystemVersion = json(&call(port, "POST", BASE, Some(&ana), None).await);
        let edit = |document: &Document, generation: i64| {
            serde_json::to_string(&DocumentEdit {
                generation,
                document: document.clone(),
            })
            .unwrap()
        };
        let path = format!("{BASE}/{}/document", draft.id);
        let bad = Document {
            accent: "#f0f; };".into(),
            ..draft.document.clone()
        };
        let refused = call(port, "PUT", &path, Some(&ana), Some(&edit(&bad, 0))).await;
        assert_eq!(
            status(&refused),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}"
        );

        let renamed = Document {
            name: "Renamed".into(),
            ..draft.document.clone()
        };
        let saved = call(port, "PUT", &path, Some(&ana), Some(&edit(&renamed, 0))).await;
        assert_eq!(status(&saved), StatusCode::OK, "{saved}");
        let saved: SystemVersion = json(&saved);
        assert_eq!(saved.document.name, "Renamed");
        assert_eq!(saved.generation, 1);
        let stale = call(port, "PUT", &path, Some(&ana), Some(&edit(&renamed, 0))).await;
        assert_eq!(status(&stale), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_version_is_validated_before_review_and_approved_before_adoption() {
        let (port, state, [ana, ..]) = setup().await;
        let draft: SystemVersion = json(&call(port, "POST", BASE, Some(&ana), None).await);

        let early = act_on(
            port,
            &ana,
            &draft.id,
            r#"{"action":"submit","generation":0}"#,
        )
        .await;
        assert_eq!(status(&early), StatusCode::CONFLICT, "unvalidated: {early}");
        let comment = act_on(
            port,
            &ana,
            &draft.id,
            r#"{"action":"comment","generation":0,"note":"x"}"#,
        )
        .await;
        assert_eq!(
            status(&comment),
            StatusCode::CONFLICT,
            "drafts take no comments"
        );

        pass(&state, &draft, None).await;
        let ready = fetch(port, &ana, &draft.id).await;
        assert!(ready.ready);
        let sent = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"submit","generation":{}}}"#, ready.generation),
        )
        .await;
        assert_eq!(status(&sent), StatusCode::OK, "{sent}");
        let sent: SystemVersion = json(&sent);
        assert_eq!(sent.phase, SystemPhase::Review);

        let not_yet = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"adopt","generation":{}}}"#, sent.generation),
        )
        .await;
        assert_eq!(
            status(&not_yet),
            StatusCode::CONFLICT,
            "adoption needs approval"
        );
        let blank = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"changes","generation":{}}}"#, sent.generation),
        )
        .await;
        assert_eq!(
            status(&blank),
            StatusCode::CONFLICT,
            "changes need a reason"
        );
        let approved: SystemVersion = json(
            &act_on(
                port,
                &ana,
                &draft.id,
                &format!(r#"{{"action":"approve","generation":{}}}"#, sent.generation),
            )
            .await,
        );
        assert_eq!(approved.phase, SystemPhase::Approved);
        adopt_and_vary(port, &ana, &draft, &approved).await;
    }

    async fn adopt_and_vary(port: u16, ana: &str, draft: &SystemVersion, approved: &SystemVersion) {
        let adopted = act_on(
            port,
            ana,
            &draft.id,
            &format!(
                r#"{{"action":"adopt","generation":{}}}"#,
                approved.generation
            ),
        )
        .await;
        assert_eq!(status(&adopted), StatusCode::OK, "{adopted}");
        let adopted: SystemVersion = json(&adopted);
        assert_eq!(
            adopted.notes.last().map(|n| n.action.as_str()),
            Some("adopt")
        );

        let again = act_on(
            port,
            ana,
            &draft.id,
            &format!(
                r#"{{"action":"adopt","generation":{}}}"#,
                adopted.generation
            ),
        )
        .await;
        assert_eq!(
            status(&again),
            StatusCode::OK,
            "adopting twice changes nothing"
        );
        assert_eq!(
            json::<SystemVersion>(&again).notes.len(),
            adopted.notes.len()
        );
        let listed: SystemList = json(&call(port, "GET", BASE, Some(ana), None).await);
        assert_eq!(listed.active.as_deref(), Some(draft.id.as_str()));

        let next: SystemVersion = json(&call(port, "POST", BASE, Some(ana), None).await);
        assert_eq!(next.parent.as_deref(), Some(draft.id.as_str()));
        let varied: SystemVersion = json(
            &act_on(
                port,
                ana,
                &next.id,
                r#"{"action":"variation","generation":0}"#,
            )
            .await,
        );
        assert_eq!(varied.parent.as_deref(), Some(next.id.as_str()));
    }
}
