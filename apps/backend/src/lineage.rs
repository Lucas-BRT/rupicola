//! Reading and editing the parents of a capture. The record keeps the parents it was published
//! with; the lineage that holds now lives in `capture_parents`, and every edit leaves an event.

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    projects::{Access, access, audit, person},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
};
use rupicola_api::{Lineage, LineageEvent, ParentEdge, ParentsEdit, Role, record::ParentRef};
use std::collections::HashSet;

/// Longest reason an edit may give, in characters.
const MAX_REASON_CHARS: usize = 500;
/// The error the database raises when an edge would close a loop.
const CYCLE_ERROR: &str = "capture lineage cycle";

/// The routes of the lineage.
pub fn routes() -> axum::Router<Shared> {
    axum::Router::new().route("/captures/{id}/parents", axum::routing::get(get).put(edit))
}

/// The project of a live capture, after checking the caller may act there as `role`.
async fn project_of(
    state: &Shared,
    principal: &Principal,
    capture: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project: String =
        sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1 AND deleted_at IS NULL")
            .bind(capture)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    match access(state, principal, &project).await? {
        Access::Device if role == Role::Reader => {}
        Access::Device => return Err(ApiError::Forbidden),
        person @ Access::Person(_) => person.require(role)?,
    }
    Ok(project)
}

/// The lineage of `capture` as it stands.
async fn load(state: &Shared, capture: &str) -> Result<Lineage, ApiError> {
    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT parent_id, kind, origin FROM capture_parents WHERE capture_id = ?1
         ORDER BY position, parent_id",
    )
    .bind(capture)
    .fetch_all(&state.db)
    .await?;
    let generation: i64 =
        sqlx::query_scalar("SELECT parents_generation FROM captures WHERE id = ?1")
            .bind(capture)
            .fetch_one(&state.db)
            .await?;
    let events: Vec<(i64, String, String, String, String, i64)> = sqlx::query_as(
        "SELECT generation, actor, reason, before, after, created_at
         FROM capture_parent_events WHERE capture_id = ?1 ORDER BY generation",
    )
    .bind(capture)
    .fetch_all(&state.db)
    .await?;
    let corrupt = |e: serde_json::Error| ApiError::Internal(e.into());
    Ok(Lineage {
        parents: edges
            .into_iter()
            .map(|(id, kind, origin)| {
                Ok(ParentEdge {
                    id,
                    kind: kind.parse().map_err(anyhow::Error::from)?,
                    origin: origin.parse().map_err(anyhow::Error::from)?,
                })
            })
            .collect::<Result<_, ApiError>>()?,
        generation,
        history: events
            .into_iter()
            .map(|(generation, actor, reason, before, after, created_at)| {
                Ok(LineageEvent {
                    generation,
                    actor,
                    reason,
                    before: serde_json::from_str(&before).map_err(corrupt)?,
                    after: serde_json::from_str(&after).map_err(corrupt)?,
                    created_at,
                })
            })
            .collect::<Result<_, ApiError>>()?,
    })
}

/// `GET /v1/captures/{id}/parents`: the parents of a capture, where each came from, and the
/// history of its edits.
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
) -> Result<Json<Lineage>, ApiError> {
    project_of(&state, &principal, &capture, Role::Reader).await?;
    Ok(Json(load(&state, &capture).await?))
}

/// `PUT /v1/captures/{id}/parents`: replaces the parents of a capture, if the lineage is still
/// the generation the edit read. Every parent must be a live capture of the same project, and
/// no parent may make the lineage a loop.
pub async fn edit(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
    Json(form): Json<ParentsEdit>,
) -> Result<Json<Lineage>, ApiError> {
    let project = project_of(&state, &principal, &capture, Role::Reviewer).await?;
    let actor = person(&principal)?.to_owned();
    let reason = check(&state, &project, &capture, &form).await?;
    let before: Vec<ParentRef> = load(&state, &capture)
        .await?
        .parents
        .into_iter()
        .map(|edge| ParentRef {
            id: edge.id,
            kind: edge.kind,
        })
        .collect();
    let mut tx = state.db.begin().await?;
    let moved = sqlx::query(
        "UPDATE captures SET parents_generation = parents_generation + 1
         WHERE id = ?1 AND parents_generation = ?2",
    )
    .bind(&capture)
    .bind(form.generation)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if moved == 0 {
        return Err(ApiError::Conflict(
            "the lineage changed since it was read; read it again",
        ));
    }
    replace(&mut tx, &project, &capture, &form.parents).await?;
    sqlx::query(
        "INSERT INTO capture_parent_events (id, capture_id, generation, actor, reason, before,
           after, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(&capture)
    .bind(form.generation + 1)
    .bind(&actor)
    .bind(&reason)
    .bind(serde_json::to_string(&before).map_err(anyhow::Error::from)?)
    .bind(serde_json::to_string(&form.parents).map_err(anyhow::Error::from)?)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Changed the parents of capture {capture}"),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load(&state, &capture).await?))
}

/// Checks an edit before anything is written; returns its trimmed reason.
async fn check(
    state: &Shared,
    project: &str,
    capture: &str,
    form: &ParentsEdit,
) -> Result<String, ApiError> {
    let reason = form.reason.trim();
    if !(1..=MAX_REASON_CHARS).contains(&reason.chars().count()) {
        return Err(ApiError::invalid(format!(
            "say why the lineage changes, in at most {MAX_REASON_CHARS} characters"
        )));
    }
    if form.parents.iter().any(|parent| parent.id == capture) {
        return Err(ApiError::invalid("a capture cannot be its own parent"));
    }
    let mut seen = HashSet::new();
    if form
        .parents
        .iter()
        .any(|parent| !seen.insert(parent.id.as_str()))
    {
        return Err(ApiError::invalid("a parent is listed twice"));
    }
    let ids: Vec<&str> = form
        .parents
        .iter()
        .map(|parent| parent.id.as_str())
        .collect();
    let found: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM captures WHERE project_id = ?1 AND deleted_at IS NULL
           AND id IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(serde_json::to_string(&ids).map_err(anyhow::Error::from)?)
    .fetch_all(&state.db)
    .await?;
    let absent: Vec<&str> = ids
        .into_iter()
        .filter(|id| !found.iter().any(|known| known == id))
        .collect();
    if absent.is_empty() {
        Ok(reason.to_owned())
    } else {
        Err(ApiError::invalid(format!(
            "these are not captures of the project: {}",
            absent.join(", ")
        )))
    }
}

/// Puts `parents` in place of the capture's edges; a parent that would close a loop refuses
/// the whole edit.
async fn replace(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    capture: &str,
    parents: &[ParentRef],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM capture_parents WHERE capture_id = ?1")
        .bind(capture)
        .execute(&mut *tx)
        .await?;
    for (position, parent) in parents.iter().enumerate() {
        let inserted = sqlx::query(
            "INSERT INTO capture_parents (project_id, capture_id, parent_id, kind, position, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, 'edit')",
        )
        .bind(project)
        .bind(capture)
        .bind(&parent.id)
        .bind(parent.kind.as_str())
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await;
        if let Err(error) = inserted {
            return Err(if error.to_string().contains(CYCLE_ERROR) {
                ApiError::invalid(format!(
                    "{} descends from this capture, so it cannot be its parent",
                    parent.id
                ))
            } else {
                error.into()
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        Lineage, ParentOrigin,
        record::{ParentKind, ParentRef},
    };

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000a";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000b";
    const C: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000c";

    fn parent(id: &str, kind: ParentKind) -> ParentRef {
        ParentRef {
            id: id.to_owned(),
            kind,
        }
    }

    /// Ana owns `demo` with captures A, B (child of A) and C; Bruno reads; Carla is outside.
    async fn demo() -> (u16, [String; 3]) {
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
        for (id, parents) in [
            (A, Vec::new()),
            (B, vec![parent(A, ParentKind::Previous)]),
            (C, Vec::new()),
        ] {
            let published = publish_record(port, &ana, &capture_record(id, &sha, 6, parents)).await;
            assert_eq!(status(&published), StatusCode::CREATED, "{published}");
        }
        (port, [ana, bruno, carla])
    }

    async fn edit(
        port: u16,
        token: &str,
        capture: &str,
        parents: &[ParentRef],
        generation: i64,
        reason: &str,
    ) -> String {
        let body =
            serde_json::json!({ "parents": parents, "generation": generation, "reason": reason });
        call(
            port,
            "PUT",
            &format!("/v1/captures/{capture}/parents"),
            Some(token),
            Some(&body.to_string()),
        )
        .await
    }

    async fn lineage(port: u16, token: &str, capture: &str) -> String {
        call(
            port,
            "GET",
            &format!("/v1/captures/{capture}/parents"),
            Some(token),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn the_record_parents_can_be_replaced_and_the_change_is_remembered() {
        let (port, [ana, bruno, _]) = demo().await;
        let read: Lineage = json(&lineage(port, &bruno, B).await);
        assert_eq!(read.parents.len(), 1);
        assert_eq!(
            (read.parents[0].id.as_str(), read.parents[0].origin),
            (A, ParentOrigin::Record)
        );
        assert_eq!((read.generation, read.history.len()), (0, 0));
        let edited = edit(
            port,
            &ana,
            B,
            &[parent(C, ParentKind::Baseline)],
            0,
            "compared against C",
        )
        .await;
        assert_eq!(status(&edited), StatusCode::OK, "{edited}");
        let after: Lineage = json(&edited);
        assert_eq!(after.parents[0].id, C);
        assert_eq!(after.parents[0].kind, ParentKind::Baseline);
        assert_eq!(after.parents[0].origin, ParentOrigin::Edit);
        assert_eq!(after.generation, 1);
        let event = &after.history[0];
        assert_eq!(
            (event.actor.as_str(), event.reason.as_str()),
            ("ana", "compared against C")
        );
        assert_eq!(event.before, [parent(A, ParentKind::Previous)]);
        assert_eq!(event.after, [parent(C, ParentKind::Baseline)]);
        let stale = edit(port, &ana, B, &[], 0, "again from the old read").await;
        assert_eq!(status(&stale), StatusCode::CONFLICT, "{stale}");
        let cleared = edit(port, &ana, B, &[], 1, "no parent after all").await;
        assert_eq!(json::<Lineage>(&cleared).parents.len(), 0);
    }

    #[tokio::test]
    async fn an_edit_cannot_make_a_loop_or_point_outside_the_project() {
        let (port, [ana, ..]) = demo().await;
        let looping = edit(port, &ana, A, &[parent(B, ParentKind::Previous)], 0, "loop").await;
        assert_eq!(
            status(&looping),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{looping}"
        );
        assert!(body(&looping).contains("descends"), "{looping}");
        let unchanged: Lineage = json(&lineage(port, &ana, A).await);
        assert_eq!(unchanged.generation, 0, "a refused edit changes nothing");
        for (parents, why) in [
            (vec![parent(A, ParentKind::Previous)], "own parent"),
            (
                vec![parent(C, ParentKind::Input), parent(C, ParentKind::Input)],
                "twice",
            ),
            (vec![parent("nope", ParentKind::Input)], "nope"),
        ] {
            let response = edit(port, &ana, A, &parents, 0, "test").await;
            assert_eq!(
                status(&response),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{response}"
            );
            assert!(body(&response).contains(why), "{why}: {response}");
        }
        let silent = edit(port, &ana, A, &[], 0, "  ").await;
        assert_eq!(status(&silent), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn readers_see_the_lineage_and_only_reviewers_change_it() {
        let (port, [_, bruno, carla]) = demo().await;
        let reader = edit(port, &bruno, B, &[], 0, "not mine to change").await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
        let outsider = lineage(port, &carla, B).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
        let missing = lineage(port, &bruno, "no-such-capture").await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND);
    }
}
