//! Problems found in a project's interface: filing, triage and discussion.

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
    Comment, NewComment, NewProblem, Problem, ProblemPatch, ProblemStatus, ProblemSummary, Role,
    Rule, Severity,
};
use std::collections::HashSet;

/// Longest problem title, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest diagnosis, proposed solution or comment, in characters.
const MAX_TEXT_CHARS: usize = 8000;
/// Longest route, viewport or theme label, in characters.
const MAX_LABEL_CHARS: usize = 256;
/// Rules one problem may cite.
const MAX_RULES: usize = 32;
/// Evidence artifacts one problem may cite.
const MAX_EVIDENCE: usize = 64;

/// Bounds on text fields, as an error message when one is out of range.
fn check_len(field: &str, value: &str, max: usize, required: bool) -> Result<(), ApiError> {
    let min = usize::from(required);
    if (min..=max).contains(&value.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "{field} must have {min}–{max} characters"
        )))
    }
}

/// Field bounds of a new problem.
fn validate(form: &NewProblem) -> Result<(), ApiError> {
    check_len("the title", form.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("the diagnosis", &form.diagnosis, MAX_TEXT_CHARS, false)?;
    check_len(
        "the proposed solution",
        &form.proposed_solution,
        MAX_TEXT_CHARS,
        false,
    )?;
    for (field, value) in [
        ("the route", &form.route),
        ("the viewport", &form.viewport),
        ("the theme", &form.theme),
    ] {
        check_len(field, value, MAX_LABEL_CHARS, false)?;
    }
    if form.rules.len() > MAX_RULES || form.evidence.len() > MAX_EVIDENCE {
        return Err(ApiError::invalid(format!(
            "a problem takes at most {MAX_RULES} rules and {MAX_EVIDENCE} evidence files"
        )));
    }
    Ok(())
}

/// Project of a problem, checked against the caller's access.
async fn problem_access(
    state: &Shared,
    principal: &Principal,
    problem: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM problems WHERE id = ?1")
            .bind(problem)
            .fetch_optional(&state.db)
            .await?;
    let project = project.ok_or(ApiError::NotFound)?;
    access(state, principal, &project).await?.require(role)?;
    Ok(project)
}

/// `POST /v1/projects/{id}/problems`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewProblem>,
) -> Result<(StatusCode, Json<Problem>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    validate(&form)?;
    let evidence: HashSet<&str> = form.evidence.iter().map(String::as_str).collect();
    for artifact in &evidence {
        let belongs: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM artifacts a JOIN captures c ON c.id = a.capture_id
             WHERE a.id = ?1 AND c.project_id = ?2",
        )
        .bind(artifact)
        .bind(&project)
        .fetch_optional(&state.db)
        .await?;
        if belongs.is_none() {
            return Err(ApiError::invalid(
                "the evidence must be files of this project's captures",
            ));
        }
    }
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO problems (id, project_id, title, status, severity, origin, route, viewport,
           theme, diagnosis, proposed_solution, author, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'open', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
    )
    .bind(&id)
    .bind(&project)
    .bind(form.title.trim())
    .bind(form.severity.as_str())
    .bind(form.origin.as_str())
    .bind(&form.route)
    .bind(&form.viewport)
    .bind(&form.theme)
    .bind(&form.diagnosis)
    .bind(&form.proposed_solution)
    .bind(&principal.handle)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    for rule in &form.rules {
        sqlx::query(
            "INSERT OR IGNORE INTO problem_rules (problem_id, scope, code, title)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&id)
        .bind(&rule.scope)
        .bind(&rule.code)
        .bind(&rule.title)
        .execute(&mut *tx)
        .await?;
    }
    for artifact in &evidence {
        sqlx::query("INSERT INTO problem_evidence (problem_id, artifact_id) VALUES (?1, ?2)")
            .bind(&id)
            .bind(artifact)
            .execute(&mut *tx)
            .await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Filed problem {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(load(&state, &id).await?)))
}

type SummaryRow = (String, String, String, String, String, String, i64);

/// `GET /v1/projects/{id}/problems`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<ProblemSummary>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT id, title, status, severity, origin, route, updated_at
         FROM problems WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    rows.into_iter()
        .map(|(id, title, status, severity, origin, route, updated_at)| {
            Ok(ProblemSummary {
                id,
                title,
                status: status.parse().map_err(anyhow::Error::from)?,
                severity: severity.parse().map_err(anyhow::Error::from)?,
                origin: origin.parse().map_err(anyhow::Error::from)?,
                route,
                updated_at,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()
        .map(Json)
}

/// `GET /v1/problems/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Problem>, ApiError> {
    problem_access(&state, &principal, &id, Role::Reader).await?;
    Ok(Json(load(&state, &id).await?))
}

/// `PATCH /v1/problems/{id}`
pub async fn update(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(patch): Json<ProblemPatch>,
) -> Result<Json<Problem>, ApiError> {
    let project = problem_access(&state, &principal, &id, Role::Reviewer).await?;
    if let Some(solution) = &patch.proposed_solution {
        check_len("the proposed solution", solution, MAX_TEXT_CHARS, false)?;
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE problems SET status = COALESCE(?2, status), severity = COALESCE(?3, severity),
           proposed_solution = COALESCE(?4, proposed_solution), updated_at = ?5
         WHERE id = ?1",
    )
    .bind(&id)
    .bind(patch.status.map(ProblemStatus::as_str))
    .bind(patch.severity.map(Severity::as_str))
    .bind(&patch.proposed_solution)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Triaged problem {id}"),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

/// `POST /v1/problems/{id}/comments`
pub async fn comment(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewComment>,
) -> Result<(StatusCode, Json<Comment>), ApiError> {
    problem_access(&state, &principal, &id, Role::Reviewer).await?;
    let text = form.text.trim();
    check_len("the comment", text, MAX_TEXT_CHARS, true)?;
    let comment = Comment {
        id: uuid::Uuid::now_v7().to_string(),
        author: principal.handle,
        text: text.to_owned(),
        created_at: now_ms(),
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO problem_comments (id, problem_id, author, text, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&comment.id)
    .bind(&id)
    .bind(&comment.author)
    .bind(&comment.text)
    .bind(comment.created_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE problems SET updated_at = ?2 WHERE id = ?1")
        .bind(&id)
        .bind(comment.created_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

type ProblemRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    i64,
);

async fn load(state: &Shared, id: &str) -> Result<Problem, ApiError> {
    let row: Option<ProblemRow> = sqlx::query_as(
        "SELECT project_id, title, status, severity, origin, route, viewport, theme, diagnosis,
           proposed_solution, author, created_at, updated_at
         FROM problems WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (
        project_id,
        title,
        status,
        severity,
        origin,
        route,
        viewport,
        theme,
        diagnosis,
        proposed_solution,
        author,
        created_at,
        updated_at,
    ) = row.ok_or(ApiError::NotFound)?;
    let rules: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT scope, code, title FROM problem_rules WHERE problem_id = ?1 ORDER BY scope, code",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let evidence: Vec<String> = sqlx::query_scalar(
        "SELECT artifact_id FROM problem_evidence WHERE problem_id = ?1 ORDER BY artifact_id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let comments: Vec<(String, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT id, author, text, created_at FROM problem_comments
         WHERE problem_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Problem {
        id: id.to_owned(),
        project_id,
        title,
        status: status.parse().map_err(anyhow::Error::from)?,
        severity: severity.parse().map_err(anyhow::Error::from)?,
        origin: origin.parse().map_err(anyhow::Error::from)?,
        route,
        viewport,
        theme,
        diagnosis,
        proposed_solution,
        rules: rules
            .into_iter()
            .map(|(scope, code, title)| Rule { scope, code, title })
            .collect(),
        evidence,
        comments: comments
            .into_iter()
            .map(|(id, author, text, created_at)| Comment {
                id,
                author,
                text,
                created_at,
            })
            .collect(),
        author,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Comment, Problem, ProblemStatus, ProblemSummary, Severity};

    /// Ana (owner), Bruno (reader) and Carla (outsider); `demo` holds one capture
    /// with the artifact `shot`.
    const USERS: usize = 3;
    const PROJECT_ARTIFACT: &str = "shot";
    const FOREIGN_ARTIFACT: &str = "foreign-shot";
    const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    async fn insert_capture(db: &sqlx::SqlitePool, project: &str, artifact: &str) {
        let capture = format!("capture-{artifact}");
        sqlx::query(
            "INSERT INTO captures (id, project_id, scenario, status, inspection, environment, created_at)
             VALUES (?1, ?2, 'home', 'complete', 'pending', '{}', 0)",
        )
        .bind(&capture)
        .bind(project)
        .execute(db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes, mime)
             VALUES (?1, ?2, 'checkpoint', 'home', 0, 'home.png', ?3, 1, 'image/png')",
        )
        .bind(artifact)
        .bind(&capture)
        .bind(DIGEST)
        .execute(db)
        .await
        .unwrap();
    }

    async fn demo() -> (u16, [String; USERS]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, carla] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        for (project, owner) in [("demo", &ana), ("other", &carla)] {
            call(
                port,
                "POST",
                "/v1/projects",
                Some(owner),
                Some(&format!(r#"{{"id":"{project}","label":"{project}"}}"#)),
            )
            .await;
        }
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        insert_capture(&state.db, "demo", PROJECT_ARTIFACT).await;
        insert_capture(&state.db, "other", FOREIGN_ARTIFACT).await;
        (port, [ana, bruno, carla])
    }

    fn new_problem(evidence: &str) -> String {
        format!(
            r#"{{"title":"Low contrast","severity":"major","origin":"agent-measured",
                "route":"/home","diagnosis":"ratio 2.1",
                "rules":[{{"scope":"core","code":"contrast","title":"Contrast"}}],
                "evidence":[{evidence}]}}"#
        )
    }

    #[tokio::test]
    async fn a_problem_is_filed_triaged_and_discussed() {
        let (port, [ana, bruno, _]) = demo().await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(&new_problem(&format!("\"{PROJECT_ARTIFACT}\""))),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let problem: Problem = json(&created);
        assert_eq!(problem.status, ProblemStatus::Open);
        assert_eq!(problem.evidence, [PROJECT_ARTIFACT]);
        assert_eq!(problem.rules[0].code, "contrast");

        let path = format!("/v1/problems/{}", problem.id);
        let triaged = call(
            port,
            "PATCH",
            &path,
            Some(&ana),
            Some(r#"{"status":"in-progress","severity":"critical"}"#),
        )
        .await;
        assert_eq!(status(&triaged), StatusCode::OK, "{triaged}");
        let triaged: Problem = json(&triaged);
        assert_eq!(triaged.status, ProblemStatus::InProgress);
        assert_eq!(triaged.severity, Severity::Critical);
        assert_eq!(triaged.diagnosis, "ratio 2.1");

        let comment = call(
            port,
            "POST",
            &format!("{path}/comments"),
            Some(&ana),
            Some(r#"{"text":"on it"}"#),
        )
        .await;
        assert_eq!(status(&comment), StatusCode::CREATED, "{comment}");
        assert_eq!(json::<Comment>(&comment).author.as_deref(), Some("ana"));

        let listed: Vec<ProblemSummary> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/problems",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, ProblemStatus::InProgress);
        let read: Problem = json(&call(port, "GET", &path, Some(&bruno), None).await);
        assert_eq!(read.comments.len(), 1);
    }

    #[tokio::test]
    async fn readers_cannot_change_and_outsiders_cannot_see() {
        let (port, [ana, bruno, carla]) = demo().await;
        let denied = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&bruno),
            Some(&new_problem("")),
        )
        .await;
        assert_eq!(status(&denied), StatusCode::FORBIDDEN, "{denied}");
        let created: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(&new_problem("")),
            )
            .await,
        );
        let path = format!("/v1/problems/{}", created.id);
        let patched = call(
            port,
            "PATCH",
            &path,
            Some(&bruno),
            Some(r#"{"status":"resolved"}"#),
        )
        .await;
        assert_eq!(status(&patched), StatusCode::FORBIDDEN, "{patched}");
        let hidden = call(port, "GET", &path, Some(&carla), None).await;
        assert_eq!(status(&hidden), StatusCode::NOT_FOUND, "{hidden}");
    }

    #[tokio::test]
    async fn evidence_must_belong_to_the_project() {
        let (port, [ana, ..]) = demo().await;
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(&new_problem(&format!("\"{FOREIGN_ARTIFACT}\""))),
        )
        .await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
    }
}
