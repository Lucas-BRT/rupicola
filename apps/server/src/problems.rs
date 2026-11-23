//! Problems found in a project's interface: filing, triage and discussion.

use crate::{
    domain::Access,
    domain::ApiError,
    domain::{Principal, secret::now_ms},
    services::access::access,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    Comment, NewComment, NewProblem, NewSelection, Problem, ProblemPatch, ProblemStatus,
    ProblemSummary, Role, Rule, Selection, SelectionItem, SelectionMark, Severity,
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
/// Selections a new problem may bring.
const MAX_SELECTIONS: usize = 16;
/// The label of the selection a problem's evidence files become.
const EVIDENCE_LABEL: &str = "Evidence";

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
    if form.rules.len() > MAX_RULES
        || form.evidence.len() > MAX_EVIDENCE
        || form.selections.len() > MAX_SELECTIONS
    {
        return Err(ApiError::invalid(format!(
            "a problem takes at most {MAX_RULES} rules, {MAX_EVIDENCE} evidence files and \
             {MAX_SELECTIONS} selections"
        )));
    }
    Ok(())
}

/// Project of a problem, checked against the caller's access.
async fn problem_access(
    state: &crate::state::AppState,
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
    let mut seen = HashSet::new();
    let mut anchors = Vec::new();
    for artifact in form.evidence.iter().filter(|a| seen.insert(a.as_str())) {
        let capture: String = sqlx::query_scalar(
            "SELECT a.capture_id FROM artifacts a JOIN captures c ON c.id = a.capture_id
             WHERE a.id = ?1 AND c.project_id = ?2 AND c.deleted_at IS NULL",
        )
        .bind(artifact)
        .bind(&project)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::invalid(
            "the evidence must be files of this project's captures",
        ))?;
        anchors.push(SelectionItem {
            capture,
            artifact: Some(artifact.clone()),
            path: None,
            mark: SelectionMark::Proof,
            fragment: None,
        });
    }
    let mut proofs = Vec::new();
    if !anchors.is_empty() {
        proofs.push(NewSelection {
            label: EVIDENCE_LABEL.to_owned(),
            items: anchors,
        });
    }
    proofs.extend(form.selections.iter().cloned());
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
    for (position, proof) in proofs.iter().enumerate() {
        let selection =
            crate::selections::keep(&mut tx, &project, principal.handle.as_deref(), proof).await?;
        cite(&mut tx, &project, &id, &selection, position).await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::repositories::audit::record(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Filed problem {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(load(&state, &id).await?)))
}

/// A `problems` row as listed.
#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: String,
    title: String,
    status: String,
    severity: String,
    origin: String,
    route: String,
    updated_at: i64,
}

impl TryFrom<SummaryRow> for ProblemSummary {
    type Error = ApiError;

    fn try_from(row: SummaryRow) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.id,
            title: row.title,
            status: row.status.parse().map_err(anyhow::Error::from)?,
            severity: row.severity.parse().map_err(anyhow::Error::from)?,
            origin: row.origin.parse().map_err(anyhow::Error::from)?,
            route: row.route,
            updated_at: row.updated_at,
        })
    }
}

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
        .map(ProblemSummary::try_from)
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
    crate::repositories::audit::record(
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

/// A `problems` row; its rules, evidence and comments live in their own tables.
#[derive(sqlx::FromRow)]
struct ProblemRow {
    id: String,
    project_id: String,
    title: String,
    status: String,
    severity: String,
    origin: String,
    route: String,
    viewport: String,
    theme: String,
    diagnosis: String,
    proposed_solution: String,
    author: Option<String>,
    created_at: i64,
    updated_at: i64,
}

/// A `problem_rules` row.
#[derive(sqlx::FromRow)]
struct RuleRow {
    scope: String,
    code: String,
    title: String,
}

impl From<RuleRow> for Rule {
    fn from(row: RuleRow) -> Self {
        Self {
            scope: row.scope,
            code: row.code,
            title: row.title,
        }
    }
}

/// A `problem_comments` row.
#[derive(sqlx::FromRow)]
struct CommentRow {
    id: String,
    author: Option<String>,
    text: String,
    created_at: i64,
}

impl From<CommentRow> for Comment {
    fn from(row: CommentRow) -> Self {
        Self {
            id: row.id,
            author: row.author,
            text: row.text,
            created_at: row.created_at,
        }
    }
}

impl ProblemRow {
    /// Joins the row with its rules, evidence and comments into the API type.
    fn into_problem(
        self,
        rules: Vec<RuleRow>,
        evidence: Vec<String>,
        selections: Vec<Selection>,
        comments: Vec<CommentRow>,
    ) -> Result<Problem, ApiError> {
        Ok(Problem {
            id: self.id,
            project_id: self.project_id,
            title: self.title,
            status: self.status.parse().map_err(anyhow::Error::from)?,
            severity: self.severity.parse().map_err(anyhow::Error::from)?,
            origin: self.origin.parse().map_err(anyhow::Error::from)?,
            route: self.route,
            viewport: self.viewport,
            theme: self.theme,
            diagnosis: self.diagnosis,
            proposed_solution: self.proposed_solution,
            rules: rules.into_iter().map(Rule::from).collect(),
            evidence,
            selections,
            comments: comments.into_iter().map(Comment::from).collect(),
            author: self.author,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

async fn load(state: &crate::state::AppState, id: &str) -> Result<Problem, ApiError> {
    let row: ProblemRow = sqlx::query_as(
        "SELECT id, project_id, title, status, severity, origin, route, viewport, theme, diagnosis,
           proposed_solution, author, created_at, updated_at
         FROM problems WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT scope, code, title FROM problem_rules WHERE problem_id = ?1 ORDER BY scope, code",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let cited: Vec<String> = sqlx::query_scalar(
        "SELECT selection_id FROM problem_selections WHERE problem_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let selections = crate::selections::load(&state.db, &cited).await?;
    let mut evidence: Vec<String> = selections
        .iter()
        .flat_map(Selection::proof)
        .map(str::to_owned)
        .collect();
    evidence.sort();
    evidence.dedup();
    let comments: Vec<CommentRow> = sqlx::query_as(
        "SELECT id, author, text, created_at FROM problem_comments
         WHERE problem_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    row.into_problem(rules, evidence, selections, comments)
}

/// Makes `problem` cite `selection`, at `position` among its selections.
async fn cite(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    problem: &str,
    selection: &str,
    position: usize,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT OR IGNORE INTO problem_selections (project_id, problem_id, selection_id, position)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(project)
    .bind(problem)
    .bind(selection)
    .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The selection, if it belongs to `project`.
async fn project_selection(
    state: &crate::state::AppState,
    project: &str,
    selection: &str,
) -> Result<(), ApiError> {
    let owner: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM selections WHERE id = ?1")
            .bind(selection)
            .fetch_optional(&state.db)
            .await?;
    if owner.as_deref() == Some(project) {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /v1/problems/{id}/selections/{selection}`: the problem cites one more selection.
pub async fn cite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Problem>, ApiError> {
    let project = problem_access(&state, &principal, &id, Role::Reviewer).await?;
    project_selection(&state, &project, &selection).await?;
    let mut tx = state.db.begin().await?;
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(position) + 1, 0) FROM problem_selections WHERE problem_id = ?1",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    cite(
        &mut tx,
        &project,
        &id,
        &selection,
        usize::try_from(next).unwrap_or_default(),
    )
    .await?;
    touch(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

/// `DELETE /v1/problems/{id}/selections/{selection}`: the problem stops citing a selection,
/// which stays.
pub async fn uncite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Problem>, ApiError> {
    problem_access(&state, &principal, &id, Role::Reviewer).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM problem_selections WHERE problem_id = ?1 AND selection_id = ?2")
        .bind(&id)
        .bind(&selection)
        .execute(&mut *tx)
        .await?;
    touch(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

async fn touch(tx: &mut sqlx::SqliteConnection, problem: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE problems SET updated_at = ?2 WHERE id = ?1")
        .bind(problem)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Comment, Problem, ProblemStatus, ProblemSummary, Severity};

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
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
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
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
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
        let (port, [ana, ..]) = demo_with_artifacts().await;
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
