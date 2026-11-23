//! Design records of a project.

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
use rupicola_api::{DesignEntry, NewDesignEntry, Role};

/// Longest title, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest note, in characters.
const MAX_NOTE_CHARS: usize = 8000;
/// Longest surface or reference, in characters.
const MAX_LABEL_CHARS: usize = 512;

/// A `design_entries` row.
#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    kind: String,
    status: String,
    title: String,
    note: String,
    surface: Option<String>,
    supersedes: Option<String>,
    reference: Option<String>,
    evidence: Option<String>,
    author: Option<String>,
    created_at: i64,
}

impl TryFrom<Row> for DesignEntry {
    type Error = ApiError;

    fn try_from(row: Row) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.id,
            entry: NewDesignEntry {
                kind: row.kind.parse().map_err(anyhow::Error::from)?,
                status: row.status.parse().map_err(anyhow::Error::from)?,
                title: row.title,
                note: row.note,
                surface: row.surface,
                supersedes: row.supersedes,
                reference: row.reference,
                evidence: row.evidence,
            },
            author: row.author,
            created_at: row.created_at,
        })
    }
}

fn validate(form: &NewDesignEntry) -> Result<(), ApiError> {
    let title = form.title.trim().chars().count();
    if !(1..=MAX_TITLE_CHARS).contains(&title) {
        return Err(ApiError::invalid(format!(
            "the title must have 1–{MAX_TITLE_CHARS} characters"
        )));
    }
    if form.note.chars().count() > MAX_NOTE_CHARS {
        return Err(ApiError::invalid(format!(
            "the note must have at most {MAX_NOTE_CHARS} characters"
        )));
    }
    for value in [&form.surface, &form.reference].into_iter().flatten() {
        if value.chars().count() > MAX_LABEL_CHARS {
            return Err(ApiError::invalid(format!(
                "the surface and the reference must have at most {MAX_LABEL_CHARS} characters"
            )));
        }
    }
    Ok(())
}

/// `GET /v1/projects/{id}/design`, newest first.
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<DesignEntry>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, kind, status, title, note, surface, supersedes, reference, evidence,
                author, created_at
         FROM design_entries WHERE project_id = ?1 ORDER BY created_at DESC, id DESC",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    rows.into_iter()
        .map(DesignEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

/// `POST /v1/projects/{id}/design`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewDesignEntry>,
) -> Result<(StatusCode, Json<DesignEntry>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    validate(&form)?;
    if let Some(artifact) = &form.evidence {
        crate::captures::require_project_artifact(&state, &project, artifact).await?;
    }
    if let Some(older) = &form.supersedes {
        let known: Option<String> =
            sqlx::query_scalar("SELECT id FROM design_entries WHERE id = ?1 AND project_id = ?2")
                .bind(older)
                .bind(&project)
                .fetch_optional(&state.db)
                .await?;
        if known.is_none() {
            return Err(ApiError::invalid(
                "a record can only replace one of the same project",
            ));
        }
    }
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO design_entries (id, project_id, kind, status, title, note, surface,
           supersedes, reference, evidence, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&id)
    .bind(&project)
    .bind(form.kind.as_str())
    .bind(form.status.as_str())
    .bind(form.title.trim())
    .bind(&form.note)
    .bind(&form.surface)
    .bind(&form.supersedes)
    .bind(&form.reference)
    .bind(&form.evidence)
    .bind(&principal.handle)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Recorded design {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    let row: Row = sqlx::query_as(
        "SELECT id, kind, status, title, note, surface, supersedes, reference, evidence,
                author, created_at FROM design_entries WHERE id = ?1",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row.try_into()?)))
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{DesignEntry, DesignKind, DesignStatus};

    const PATH: &str = "/v1/projects/demo/design";

    fn form(extra: &str) -> String {
        format!(r#"{{"kind":"tactic","status":"adopted","title":"Hairline seams"{extra}}}"#)
    }

    #[tokio::test]
    async fn a_record_is_kept_replaced_and_listed_newest_first() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let first = call(port, "POST", PATH, Some(&ana), Some(&form(""))).await;
        assert_eq!(status(&first), StatusCode::CREATED, "{first}");
        let first: DesignEntry = json(&first);
        assert_eq!(first.entry.kind, DesignKind::Tactic);
        assert_eq!(first.entry.status, DesignStatus::Adopted);
        assert_eq!(first.author.as_deref(), Some("ana"));

        let extra = format!(
            r#","supersedes":"{}","evidence":"{PROJECT_ARTIFACT}""#,
            first.id
        );
        let second = call(port, "POST", PATH, Some(&ana), Some(&form(&extra))).await;
        assert_eq!(status(&second), StatusCode::CREATED, "{second}");

        let listed: Vec<DesignEntry> = json(&call(port, "GET", PATH, Some(&bruno), None).await);
        assert_eq!(listed.len(), 2);
        assert_eq!(
            listed[0].entry.supersedes.as_deref(),
            Some(first.id.as_str())
        );
        assert_eq!(listed[0].entry.evidence.as_deref(), Some(PROJECT_ARTIFACT));

        assert_eq!(
            status(&call(port, "POST", PATH, Some(&bruno), Some(&form(""))).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&call(port, "GET", PATH, Some(&carla), None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_record_cannot_cite_another_projects_files_or_records() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        for extra in [
            format!(r#","evidence":"{FOREIGN_ARTIFACT}""#),
            r#","supersedes":"missing""#.to_owned(),
        ] {
            let refused = call(port, "POST", PATH, Some(&ana), Some(&form(&extra))).await;
            assert!(status(&refused).is_client_error(), "{refused}");
        }
        let blank = call(
            port,
            "POST",
            PATH,
            Some(&ana),
            Some(r#"{"kind":"brief","status":"proposed","title":"  "}"#),
        )
        .await;
        assert_eq!(status(&blank), StatusCode::UNPROCESSABLE_ENTITY, "{blank}");
    }
}
