//! Selections: frozen lists of anchors into a project's captures, which problems and fixes cite
//! as proof. A selection is never changed; a new proof is a new selection.

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    projects::{Access, access},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    Fragment, MAX_SELECTION_ITEMS, MAX_SELECTION_LABEL_CHARS, NewSelection, Role, Selection,
    SelectionItem,
};
use std::collections::HashMap;

/// The routes of the selections.
pub fn routes() -> axum::Router<Shared> {
    use axum::routing;
    axum::Router::new()
        .route("/projects/{id}/selections", routing::post(create))
        .route("/selections/{id}", routing::get(get))
}

/// Keeps `form` as a selection of `project` inside `tx`; returns its id. Every anchor must be a
/// live capture of the project, and a file one of that capture's.
///
/// # Errors
///
/// [`ApiError::Invalid`] naming the first anchor that is not.
pub async fn keep(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    author: Option<&str>,
    form: &NewSelection,
) -> Result<String, ApiError> {
    let label = form.label.trim();
    if !(1..=MAX_SELECTION_LABEL_CHARS).contains(&label.chars().count()) {
        return Err(ApiError::invalid(format!(
            "a selection needs a label of at most {MAX_SELECTION_LABEL_CHARS} characters"
        )));
    }
    if !(1..=MAX_SELECTION_ITEMS).contains(&form.items.len()) {
        return Err(ApiError::invalid(format!(
            "a selection holds 1 to {MAX_SELECTION_ITEMS} anchors"
        )));
    }
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO selections (id, project_id, label, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(project)
    .bind(label)
    .bind(author)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    for (position, item) in form.items.iter().enumerate() {
        let artifact = resolve(tx, project, item).await?;
        let fragment = item
            .fragment
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(anyhow::Error::from)?;
        sqlx::query(
            "INSERT INTO selection_items (project_id, selection_id, position, capture_id,
               artifact_id, mark, fragment)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(project)
        .bind(&id)
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .bind(&item.capture)
        .bind(artifact)
        .bind(item.mark.as_str())
        .bind(fragment)
        .execute(&mut *tx)
        .await?;
    }
    Ok(id)
}

/// The artifact an anchor names, by id or by path, after checking it belongs to the anchor's
/// capture and the capture to the project; `None` for a whole capture.
async fn resolve(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    item: &SelectionItem,
) -> Result<Option<String>, ApiError> {
    let capture: Option<String> = sqlx::query_scalar(
        "SELECT id FROM captures WHERE id = ?1 AND project_id = ?2 AND deleted_at IS NULL",
    )
    .bind(&item.capture)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    if capture.is_none() {
        return Err(ApiError::invalid(format!(
            "{} is not a capture of this project",
            item.capture
        )));
    }
    let found: Option<String> = match (&item.artifact, &item.path) {
        (None, None) => return Ok(None),
        (Some(artifact), _) => {
            sqlx::query_scalar("SELECT id FROM artifacts WHERE id = ?1 AND capture_id = ?2")
                .bind(artifact)
                .bind(&item.capture)
                .fetch_optional(&mut *tx)
                .await?
        }
        (None, Some(path)) => {
            sqlx::query_scalar("SELECT id FROM artifacts WHERE path = ?1 AND capture_id = ?2")
                .bind(path)
                .bind(&item.capture)
                .fetch_optional(&mut *tx)
                .await?
        }
    };
    found.map(Some).ok_or_else(|| {
        ApiError::invalid(format!(
            "capture {} has no file {}",
            item.capture,
            item.artifact
                .as_deref()
                .or(item.path.as_deref())
                .unwrap_or_default()
        ))
    })
}

/// A `selections` row.
#[derive(sqlx::FromRow)]
struct HeadRow {
    id: String,
    label: String,
    author: Option<String>,
    created_at: i64,
}

/// A `selection_items` row.
#[derive(sqlx::FromRow)]
struct ItemRow {
    selection_id: String,
    capture_id: String,
    artifact_id: Option<String>,
    mark: String,
    fragment: Option<String>,
}

/// The selections named in `ids`, in that order.
///
/// # Errors
///
/// When the database cannot be read or holds a malformed fragment.
pub async fn load(db: &sqlx::SqlitePool, ids: &[String]) -> Result<Vec<Selection>, ApiError> {
    let wanted = serde_json::to_string(ids).map_err(anyhow::Error::from)?;
    let heads: Vec<HeadRow> = sqlx::query_as(
        "SELECT id, label, author, created_at FROM selections
         WHERE id IN (SELECT value FROM json_each(?1))",
    )
    .bind(&wanted)
    .fetch_all(db)
    .await?;
    let rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT selection_id, capture_id, artifact_id, mark, fragment FROM selection_items
         WHERE selection_id IN (SELECT value FROM json_each(?1)) ORDER BY selection_id, position",
    )
    .bind(&wanted)
    .fetch_all(db)
    .await?;
    let mut items: HashMap<String, Vec<SelectionItem>> = HashMap::new();
    for row in rows {
        let fragment: Option<Fragment> = row
            .fragment
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(anyhow::Error::from)?;
        items
            .entry(row.selection_id)
            .or_default()
            .push(SelectionItem {
                capture: row.capture_id,
                artifact: row.artifact_id,
                path: None,
                mark: row.mark.parse().map_err(anyhow::Error::from)?,
                fragment,
            });
    }
    let mut by_id: HashMap<String, Selection> = heads
        .into_iter()
        .map(|head| {
            let anchors = items.remove(&head.id).unwrap_or_default();
            (
                head.id.clone(),
                Selection {
                    id: head.id,
                    label: head.label,
                    author: head.author,
                    created_at: head.created_at,
                    items: anchors,
                },
            )
        })
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// `POST /v1/projects/{id}/selections`: keeps a selection to cite later.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewSelection>,
) -> Result<(StatusCode, Json<Selection>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    let mut tx = state.db.begin().await?;
    let id = keep(&mut tx, &project, principal.handle.as_deref(), &form).await?;
    tx.commit().await?;
    let mut kept = load(&state.db, std::slice::from_ref(&id)).await?;
    Ok((StatusCode::CREATED, Json(kept.remove(0))))
}

/// `GET /v1/selections/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Selection>, ApiError> {
    let project: String = sqlx::query_scalar("SELECT project_id FROM selections WHERE id = ?1")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reader)?,
    }
    let mut kept = load(&state.db, std::slice::from_ref(&id)).await?;
    Ok(Json(kept.remove(0)))
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Capture, Problem, Selection, SelectionMark};
    use serde_json::json;

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000005a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000005b1";
    const PATH: &str = "checkpoints/home.png";

    /// Ana owns `demo` with captures A and B; Bruno reads; Carla owns `other` with a capture.
    /// Returns the tokens and the artifact id of A's file.
    async fn demo() -> (u16, [String; 3], String) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, carla] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
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
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        for id in [A, B] {
            publish_record(port, &ana, &capture_record(id, &sha, 6, Vec::new())).await;
        }
        let capture: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{A}"), Some(&ana), None).await);
        let artifact = capture.checkpoints[0].artifact.id.clone();
        (port, [ana, bruno, carla], artifact)
    }

    async fn select(port: u16, token: &str, body: &serde_json::Value) -> String {
        call(
            port,
            "POST",
            "/v1/projects/demo/selections",
            Some(token),
            Some(&body.to_string()),
        )
        .await
    }

    #[tokio::test]
    async fn a_selection_names_files_by_id_or_by_path_and_whole_captures() {
        let (port, [ana, bruno, _], artifact) = demo().await;
        let body = json!({
            "label": "Menu contrast",
            "items": [
                { "capture": A, "artifact": artifact, "mark": "proof", "fragment": { "rect": [0, 0, 40, 20] } },
                { "capture": B, "path": PATH, "mark": "proof" },
                { "capture": B, "mark": "context" }
            ]
        });
        let made = select(port, &ana, &body).await;
        assert_eq!(status(&made), StatusCode::CREATED, "{made}");
        let selection: Selection = json(&made);
        assert_eq!(selection.items.len(), 3);
        assert_eq!(
            selection.items[0].fragment.as_ref().unwrap().rect,
            Some([0, 0, 40, 20])
        );
        assert!(
            selection.items[1].artifact.is_some(),
            "a path comes back as the artifact id"
        );
        assert_eq!(selection.items[2].mark, SelectionMark::Context);
        assert_eq!(selection.proof().count(), 2);
        let read = call(
            port,
            "GET",
            &format!("/v1/selections/{}", selection.id),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(json::<Selection>(&read), selection);
    }

    #[tokio::test]
    async fn an_anchor_outside_the_project_or_its_capture_is_refused() {
        let (port, [ana, bruno, _], artifact) = demo().await;
        for (form, why) in [
            (
                json!({ "label": "x", "items": [{ "capture": "nope", "mark": "proof" }] }),
                "not a capture",
            ),
            (
                json!({ "label": "x", "items": [{ "capture": B, "artifact": artifact, "mark": "proof" }] }),
                "has no file",
            ),
            (
                json!({ "label": "x", "items": [{ "capture": A, "path": "missing.png", "mark": "proof" }] }),
                "has no file",
            ),
            (
                json!({ "label": " ", "items": [{ "capture": A, "mark": "proof" }] }),
                "label",
            ),
            (json!({ "label": "x", "items": [] }), "anchors"),
        ] {
            let refused = select(port, &ana, &form).await;
            assert_eq!(
                status(&refused),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{refused}"
            );
            assert!(body(&refused).contains(why), "{why}: {refused}");
        }
        let reader = select(
            port,
            &bruno,
            &json!({ "label": "x", "items": [{ "capture": A, "mark": "proof" }] }),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_problem_cites_its_evidence_and_more_selections_and_can_drop_one() {
        let (port, [ana, _, carla], artifact) = demo().await;
        let filed = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(
                &json!({
                    "title": "Low contrast", "severity": "major", "origin": "user-reported",
                    "evidence": [artifact],
                    "selections": [{ "label": "Mobile too", "items": [{ "capture": B, "path": PATH, "mark": "proof" }] }]
                })
                .to_string(),
            ),
        )
        .await;
        assert_eq!(status(&filed), StatusCode::CREATED, "{filed}");
        let problem: Problem = json(&filed);
        let labels: Vec<&str> = problem
            .selections
            .iter()
            .map(|s| s.label.as_str())
            .collect();
        assert_eq!(labels, ["Evidence", "Mobile too"]);
        assert_eq!(problem.evidence.len(), 2, "both files are proof");
        let extra: Selection = json(
            &select(
                port,
                &ana,
                &json!({ "label": "Context", "items": [{ "capture": A, "mark": "context" }] }),
            )
            .await,
        );
        let cited = call(
            port,
            "PUT",
            &format!("/v1/problems/{}/selections/{}", problem.id, extra.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(json::<Problem>(&cited).selections.len(), 3);
        let dropped = call(
            port,
            "DELETE",
            &format!(
                "/v1/problems/{}/selections/{}",
                problem.id, problem.selections[0].id
            ),
            Some(&ana),
            None,
        )
        .await;
        let after: Problem = json(&dropped);
        assert_eq!(after.selections.len(), 2);
        assert_eq!(
            after.evidence.len(),
            1,
            "the dropped selection's proof is gone"
        );
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&carla),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let foreign = call(
            port,
            "PUT",
            &format!("/v1/problems/{}/selections/nope", problem.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&foreign), StatusCode::NOT_FOUND);
    }
}
