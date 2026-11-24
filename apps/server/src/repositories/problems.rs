//! SQLite queries for `problems`, `problem_rules`, `problem_selections` and `problem_comments`.

use crate::{domain::ApiError, repositories::selections};
use rupicola_api::{
    Comment, NewProblem, Problem, ProblemPatch, ProblemStatus, ProblemSummary, Rule, Selection,
    Severity,
};
use sqlx::{SqliteConnection, SqlitePool};

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

/// The project that owns `problem`, if it exists.
pub async fn project_of(db: &SqlitePool, problem: &str) -> Result<Option<String>, ApiError> {
    let project = sqlx::query_scalar("SELECT project_id FROM problems WHERE id = ?1")
        .bind(problem)
        .fetch_optional(db)
        .await?;
    Ok(project)
}

/// The live capture that holds `artifact` in `project`, if any.
pub async fn resolve_evidence_capture(
    db: impl sqlx::SqliteExecutor<'_>,
    project: &str,
    artifact: &str,
) -> Result<Option<String>, ApiError> {
    let capture = sqlx::query_scalar(
        "SELECT a.capture_id FROM artifacts a JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2 AND c.deleted_at IS NULL",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(db)
    .await?;
    Ok(capture)
}

/// Inserts a problem row with `status` and its rules inside `tx`.
pub async fn insert_with_status(
    tx: &mut SqliteConnection,
    id: &str,
    project: &str,
    form: &NewProblem,
    status: ProblemStatus,
    author: Option<&str>,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO problems (id, project_id, title, status, severity, origin, route, viewport,
           theme, diagnosis, proposed_solution, author, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
    )
    .bind(id)
    .bind(project)
    .bind(form.title.trim())
    .bind(status.as_str())
    .bind(form.severity.as_str())
    .bind(form.origin.as_str())
    .bind(&form.route)
    .bind(&form.viewport)
    .bind(&form.theme)
    .bind(&form.diagnosis)
    .bind(&form.proposed_solution)
    .bind(author)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    for rule in &form.rules {
        sqlx::query(
            "INSERT OR IGNORE INTO problem_rules (problem_id, scope, code, title)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(id)
        .bind(&rule.scope)
        .bind(&rule.code)
        .bind(&rule.title)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Lists every problem summary of `project`, newest first.
pub async fn list(db: &SqlitePool, project: &str) -> Result<Vec<ProblemSummary>, ApiError> {
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT id, title, status, severity, origin, route, updated_at
         FROM problems WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC",
    )
    .bind(project)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(ProblemSummary::try_from).collect()
}

/// Lists every problem of `project` in full, newest first.
pub async fn list_details(db: &SqlitePool, project: &str) -> Result<Vec<Problem>, ApiError> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM problems WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC",
    )
    .bind(project)
    .fetch_all(db)
    .await?;
    let mut detailed = Vec::with_capacity(ids.len());
    for id in ids {
        detailed.push(load(db, &id).await?);
    }
    Ok(detailed)
}

/// Applies `patch` to `problem` inside `tx`.
pub async fn update(
    tx: &mut SqliteConnection,
    id: &str,
    patch: &ProblemPatch,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE problems SET status = COALESCE(?2, status), severity = COALESCE(?3, severity),
           proposed_solution = COALESCE(?4, proposed_solution), updated_at = ?5
         WHERE id = ?1",
    )
    .bind(id)
    .bind(patch.status.map(ProblemStatus::as_str))
    .bind(patch.severity.map(Severity::as_str))
    .bind(&patch.proposed_solution)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Inserts a comment on `problem` inside `tx`.
pub async fn insert_comment(
    tx: &mut SqliteConnection,
    problem: &str,
    comment: &Comment,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO problem_comments (id, problem_id, author, text, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&comment.id)
    .bind(problem)
    .bind(&comment.author)
    .bind(&comment.text)
    .bind(comment.created_at)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Updates `problems.updated_at` inside `tx`.
pub async fn touch(tx: &mut SqliteConnection, problem: &str, now: i64) -> Result<(), ApiError> {
    sqlx::query("UPDATE problems SET updated_at = ?2 WHERE id = ?1")
        .bind(problem)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Makes `problem` cite `selection`, at `position` among its selections.
pub async fn cite(
    tx: &mut SqliteConnection,
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

/// Next selection position for `problem` inside `tx`.
pub async fn next_selection_position(
    tx: &mut SqliteConnection,
    problem: &str,
) -> Result<usize, ApiError> {
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(position) + 1, 0) FROM problem_selections WHERE problem_id = ?1",
    )
    .bind(problem)
    .fetch_one(&mut *tx)
    .await?;
    Ok(usize::try_from(next).unwrap_or_default())
}

/// Removes `selection` from `problem`'s cited selections inside `tx`.
pub async fn uncite(
    tx: &mut SqliteConnection,
    problem: &str,
    selection: &str,
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM problem_selections WHERE problem_id = ?1 AND selection_id = ?2")
        .bind(problem)
        .bind(selection)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Loads one problem with its rules, selections, derived evidence and comments.
pub async fn load(db: &SqlitePool, id: &str) -> Result<Problem, ApiError> {
    let row: ProblemRow = sqlx::query_as(
        "SELECT id, project_id, title, status, severity, origin, route, viewport, theme, diagnosis,
           proposed_solution, author, created_at, updated_at
         FROM problems WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT scope, code, title FROM problem_rules WHERE problem_id = ?1 ORDER BY scope, code",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let cited: Vec<String> = sqlx::query_scalar(
        "SELECT selection_id FROM problem_selections WHERE problem_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let selections = selections::load(db, &cited).await?;
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
    .fetch_all(db)
    .await?;
    row.into_problem(rules, evidence, selections, comments)
}
