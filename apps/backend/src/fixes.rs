//! Fixes that answer problems: filing with before/after proof, steps and review.

use crate::{
    auth::{Principal, now_ms},
    captures::require_project_artifact,
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
    Comparison, Fix, FixSelection, FixSelectionLink, FixSelectionRole, FixStatus, FixSummary,
    NewFix, NewReview, NewSelection, NewStep, ProblemStatus, Review, Role, SelectionItem,
    SelectionMark, Step,
};
use std::collections::BTreeSet;

/// Longest fix title or comparison label, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest summary, step detail, verification or review comment, in characters.
const MAX_TEXT_CHARS: usize = 8000;
/// Problems, tactics, files, comparisons or steps one fix may hold of each kind.
const MAX_ITEMS: usize = 64;

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

fn check_step(step: &NewStep) -> Result<(), ApiError> {
    check_len("a step title", step.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("a step detail", &step.detail, MAX_TEXT_CHARS, false)?;
    check_len(
        "a step verification",
        &step.verification,
        MAX_TEXT_CHARS,
        false,
    )
}

/// Field bounds of a new fix.
fn validate(form: &NewFix) -> Result<(), ApiError> {
    check_len("the title", form.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("the summary", &form.summary, MAX_TEXT_CHARS, false)?;
    if form.problems.is_empty() {
        return Err(ApiError::invalid("a fix answers at least one problem"));
    }
    let largest = [
        form.problems.len(),
        form.tactics.len(),
        form.files.len(),
        form.comparisons.len(),
        form.steps.len(),
        form.baseline.len(),
        form.proof.len(),
    ]
    .into_iter()
    .max()
    .unwrap_or_default();
    if largest > MAX_ITEMS {
        return Err(ApiError::invalid(format!(
            "a fix takes at most {MAX_ITEMS} problems, tactics, files, comparisons, steps and \
             selections of each role"
        )));
    }
    for comparison in &form.comparisons {
        check_len(
            "a comparison label",
            &comparison.label,
            MAX_TITLE_CHARS,
            true,
        )?;
    }
    form.steps.iter().try_for_each(check_step)
}

/// Project of a fix, checked against the caller's access.
async fn fix_access(
    state: &Shared,
    principal: &Principal,
    fix: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project: Option<String> = sqlx::query_scalar("SELECT project_id FROM fixes WHERE id = ?1")
        .bind(fix)
        .fetch_optional(&state.db)
        .await?;
    let project = project.ok_or(ApiError::NotFound)?;
    access(state, principal, &project).await?.require(role)?;
    Ok(project)
}

/// Fails unless the fix's problems and artifacts all belong to `project`; returns the
/// distinct problem ids.
async fn check_references<'a>(
    state: &Shared,
    project: &str,
    form: &'a NewFix,
) -> Result<BTreeSet<&'a str>, ApiError> {
    let problems: BTreeSet<&str> = form.problems.iter().map(String::as_str).collect();
    for problem in &problems {
        let belongs: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM problems WHERE id = ?1 AND project_id = ?2")
                .bind(problem)
                .bind(project)
                .fetch_optional(&state.db)
                .await?;
        if belongs.is_none() {
            return Err(ApiError::invalid(
                "the problems must belong to this project",
            ));
        }
    }
    for comparison in &form.comparisons {
        for artifact in [
            Some(&comparison.before),
            Some(&comparison.after),
            comparison.diff.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            require_project_artifact(state, project, artifact).await?;
        }
    }
    for artifact in form.steps.iter().filter_map(|step| step.evidence.as_ref()) {
        require_project_artifact(state, project, artifact).await?;
    }
    Ok(problems)
}

/// Tactics, files, comparisons and steps of a new fix.
async fn insert_details(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    form: &NewFix,
    now: i64,
) -> Result<(), ApiError> {
    for tactic in &form.tactics {
        sqlx::query("INSERT OR IGNORE INTO fix_tactics (fix_id, tactic) VALUES (?1, ?2)")
            .bind(id)
            .bind(tactic)
            .execute(&mut *tx)
            .await?;
    }
    for path in &form.files {
        sqlx::query("INSERT OR IGNORE INTO fix_files (fix_id, path) VALUES (?1, ?2)")
            .bind(id)
            .bind(path)
            .execute(&mut *tx)
            .await?;
    }
    for (position, comparison) in form.comparisons.iter().enumerate() {
        sqlx::query(
            "INSERT INTO fix_comparisons (id, fix_id, position, label, route, viewport, theme,
               before_artifact_id, after_artifact_id, diff_artifact_id, changed_ratio)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(id)
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .bind(&comparison.label)
        .bind(&comparison.route)
        .bind(&comparison.viewport)
        .bind(&comparison.theme)
        .bind(&comparison.before)
        .bind(&comparison.after)
        .bind(&comparison.diff)
        .bind(comparison.changed_ratio)
        .execute(&mut *tx)
        .await?;
    }
    for step in &form.steps {
        insert_step(tx, id, step, now).await?;
    }
    Ok(())
}

/// `POST /v1/projects/{id}/fixes`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewFix>,
) -> Result<(StatusCode, Json<Fix>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    validate(&form)?;
    let problems = check_references(&state, &project, &form).await?;
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO fixes (id, project_id, title, summary, status, author, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'pending-review', ?5, ?6, ?6)",
    )
    .bind(&id)
    .bind(&project)
    .bind(form.title.trim())
    .bind(&form.summary)
    .bind(&principal.handle)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    for problem in &problems {
        sqlx::query("INSERT INTO fix_problems (fix_id, problem_id) VALUES (?1, ?2)")
            .bind(&id)
            .bind(problem)
            .execute(&mut *tx)
            .await?;
        set_problem_status(&mut tx, problem, ProblemStatus::InProgress, now).await?;
    }
    insert_details(&mut tx, &id, &form, now).await?;
    for (position, (role, proof)) in proofs(&mut tx, &project, &form)
        .await?
        .into_iter()
        .enumerate()
    {
        let selection =
            crate::selections::keep(&mut tx, &project, principal.handle.as_deref(), &proof).await?;
        let position = i64::try_from(position).map_err(anyhow::Error::from)?;
        cite(&mut tx, &project, &id, &selection, role, position).await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Filed fix {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(load(&state, &id).await?)))
}

/// Label of the selection a fix's comparisons make of their before files, and of their after
/// and diff files.
const BEFORE_LABEL: &str = "Before";
const AFTER_LABEL: &str = "After";

/// The selections a new fix cites: its comparisons' before files as the baseline and their
/// after and diff files as proof, then any selections it brings.
async fn proofs(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    form: &NewFix,
) -> Result<Vec<(FixSelectionRole, NewSelection)>, ApiError> {
    let mut before = Vec::new();
    let mut after = Vec::new();
    for comparison in &form.comparisons {
        before.push(anchor(tx, project, &comparison.before).await?);
        after.push(anchor(tx, project, &comparison.after).await?);
        if let Some(diff) = &comparison.diff {
            after.push(anchor(tx, project, diff).await?);
        }
    }
    let mut all = Vec::new();
    for (role, label, items) in [
        (FixSelectionRole::Baseline, BEFORE_LABEL, before),
        (FixSelectionRole::SolutionProof, AFTER_LABEL, after),
    ] {
        if !items.is_empty() {
            all.push((
                role,
                NewSelection {
                    label: label.to_owned(),
                    items,
                },
            ));
        }
    }
    all.extend(
        form.baseline
            .iter()
            .cloned()
            .map(|s| (FixSelectionRole::Baseline, s)),
    );
    all.extend(
        form.proof
            .iter()
            .cloned()
            .map(|s| (FixSelectionRole::SolutionProof, s)),
    );
    Ok(all)
}

/// An anchor of proof at an artifact the caller already checked belongs to the project.
async fn anchor(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    artifact: &str,
) -> Result<SelectionItem, ApiError> {
    let capture: String = sqlx::query_scalar(
        "SELECT a.capture_id FROM artifacts a JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::invalid(
        "the comparisons must use files of this project",
    ))?;
    Ok(SelectionItem {
        capture,
        artifact: Some(artifact.to_owned()),
        path: None,
        mark: SelectionMark::Proof,
        fragment: None,
    })
}

/// Makes `fix` cite `selection` as `role`, at `position`.
async fn cite(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    fix: &str,
    selection: &str,
    role: FixSelectionRole,
    position: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO fix_selections (project_id, fix_id, selection_id, role, position)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(fix_id, selection_id) DO UPDATE SET role = excluded.role",
    )
    .bind(project)
    .bind(fix)
    .bind(selection)
    .bind(role.as_str())
    .bind(position)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The routes that make a fix cite a selection, or stop citing it.
pub fn routes() -> axum::Router<Shared> {
    axum::Router::new().route(
        "/fixes/{id}/selections/{selection}",
        axum::routing::put(cite_selection).delete(uncite_selection),
    )
}

/// `PUT /v1/fixes/{id}/selections/{selection}`: the fix cites a selection of the same project
/// as its baseline or as proof that it works.
pub async fn cite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
    Json(link): Json<FixSelectionLink>,
) -> Result<Json<Fix>, ApiError> {
    let project = fix_access(&state, &principal, &id, Role::Reviewer).await?;
    let owner: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM selections WHERE id = ?1")
            .bind(&selection)
            .fetch_optional(&state.db)
            .await?;
    if owner.as_deref() != Some(project.as_str()) {
        return Err(ApiError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(position) + 1, 0) FROM fix_selections WHERE fix_id = ?1",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    cite(&mut tx, &project, &id, &selection, link.role, next).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

/// `DELETE /v1/fixes/{id}/selections/{selection}`: the fix stops citing a selection, which
/// stays.
pub async fn uncite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Fix>, ApiError> {
    fix_access(&state, &principal, &id, Role::Reviewer).await?;
    sqlx::query("DELETE FROM fix_selections WHERE fix_id = ?1 AND selection_id = ?2")
        .bind(&id)
        .bind(&selection)
        .execute(&state.db)
        .await?;
    Ok(Json(load(&state, &id).await?))
}

async fn set_problem_status(
    tx: &mut sqlx::SqliteConnection,
    problem: &str,
    status: ProblemStatus,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE problems SET status = ?2, updated_at = ?3 WHERE id = ?1")
        .bind(problem)
        .bind(status.as_str())
        .bind(now)
        .execute(tx)
        .await?;
    Ok(())
}

async fn insert_step(
    tx: &mut sqlx::SqliteConnection,
    fix: &str,
    step: &NewStep,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO fix_steps (id, fix_id, position, title, detail, verification,
           evidence_artifact_id, created_at)
         VALUES (?1, ?2, (SELECT COUNT(*) FROM fix_steps WHERE fix_id = ?2), ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(fix)
    .bind(step.title.trim())
    .bind(&step.detail)
    .bind(&step.verification)
    .bind(&step.evidence)
    .bind(now)
    .execute(tx)
    .await?;
    Ok(())
}

/// `GET /v1/projects/{id}/fixes`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<FixSummary>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT f.id, f.title, f.status,
           (SELECT COUNT(*) FROM fix_problems p WHERE p.fix_id = f.id) AS problems, f.updated_at
         FROM fixes f WHERE f.project_id = ?1 ORDER BY f.updated_at DESC, f.id DESC",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    rows.into_iter()
        .map(FixSummary::try_from)
        .collect::<Result<Vec<_>, ApiError>>()
        .map(Json)
}

/// `GET /v1/fixes/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Fix>, ApiError> {
    fix_access(&state, &principal, &id, Role::Reader).await?;
    Ok(Json(load(&state, &id).await?))
}

/// `POST /v1/fixes/{id}/steps`: records one more verified step of the work.
pub async fn add_step(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(step): Json<NewStep>,
) -> Result<(StatusCode, Json<Step>), ApiError> {
    let project = fix_project(&state, &id).await?;
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    check_step(&step)?;
    if let Some(artifact) = &step.evidence {
        require_project_artifact(&state, &project, artifact).await?;
    }
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    insert_step(&mut tx, &id, &step, now).await?;
    sqlx::query("UPDATE fixes SET updated_at = ?2 WHERE id = ?1")
        .bind(&id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Step {
            step,
            created_at: now,
        }),
    ))
}

async fn fix_project(state: &Shared, fix: &str) -> Result<String, ApiError> {
    sqlx::query_scalar("SELECT project_id FROM fixes WHERE id = ?1")
        .bind(fix)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// `POST /v1/fixes/{id}/reviews`
pub async fn review(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewReview>,
) -> Result<Json<Fix>, ApiError> {
    let project = fix_access(&state, &principal, &id, Role::Reviewer).await?;
    check_len("the comment", &form.comment, MAX_TEXT_CHARS, false)?;
    let problem_status = match form.outcome {
        FixStatus::PendingReview => {
            return Err(ApiError::invalid(
                "a review approves, requests changes or supersedes",
            ));
        }
        FixStatus::Approved => Some(ProblemStatus::Resolved),
        FixStatus::ChangesRequested => Some(ProblemStatus::InProgress),
        FixStatus::Superseded => None,
    };
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO fix_reviews (id, fix_id, author, outcome, comment, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(&id)
    .bind(&principal.handle)
    .bind(form.outcome.as_str())
    .bind(&form.comment)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE fixes SET status = ?2, updated_at = ?3 WHERE id = ?1")
        .bind(&id)
        .bind(form.outcome.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if let Some(status) = problem_status {
        let problems: Vec<String> =
            sqlx::query_scalar("SELECT problem_id FROM fix_problems WHERE fix_id = ?1")
                .bind(&id)
                .fetch_all(&mut *tx)
                .await?;
        for problem in problems {
            set_problem_status(&mut tx, &problem, status, now).await?;
        }
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Reviewed fix {id}: {}", form.outcome),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

/// A `fixes` row as listed.
#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: String,
    title: String,
    status: String,
    problems: i64,
    updated_at: i64,
}

impl TryFrom<SummaryRow> for FixSummary {
    type Error = ApiError;

    fn try_from(row: SummaryRow) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.id,
            title: row.title,
            status: row.status.parse().map_err(anyhow::Error::from)?,
            problems: usize::try_from(row.problems).map_err(anyhow::Error::from)?,
            updated_at: row.updated_at,
        })
    }
}

/// A `fixes` row; its problems, tactics, files, comparisons, steps and reviews live in their
/// own tables.
#[derive(sqlx::FromRow)]
struct FixRow {
    project_id: String,
    title: String,
    summary: String,
    status: String,
    author: Option<String>,
    created_at: i64,
    updated_at: i64,
}

/// A `fix_comparisons` row.
#[derive(sqlx::FromRow)]
struct ComparisonRow {
    label: String,
    route: String,
    viewport: String,
    theme: String,
    before_artifact_id: String,
    after_artifact_id: String,
    diff_artifact_id: Option<String>,
    changed_ratio: Option<f64>,
}

impl From<ComparisonRow> for Comparison {
    fn from(row: ComparisonRow) -> Self {
        Self {
            label: row.label,
            route: row.route,
            viewport: row.viewport,
            theme: row.theme,
            before: row.before_artifact_id,
            after: row.after_artifact_id,
            diff: row.diff_artifact_id,
            changed_ratio: row.changed_ratio,
        }
    }
}

/// A `fix_steps` row.
#[derive(sqlx::FromRow)]
struct StepRow {
    title: String,
    detail: String,
    verification: String,
    evidence_artifact_id: Option<String>,
    created_at: i64,
}

impl From<StepRow> for Step {
    fn from(row: StepRow) -> Self {
        Self {
            step: NewStep {
                title: row.title,
                detail: row.detail,
                verification: row.verification,
                evidence: row.evidence_artifact_id,
            },
            created_at: row.created_at,
        }
    }
}

/// A `fix_reviews` row.
#[derive(sqlx::FromRow)]
struct ReviewRow {
    author: Option<String>,
    outcome: String,
    comment: String,
    created_at: i64,
}

impl TryFrom<ReviewRow> for Review {
    type Error = ApiError;

    fn try_from(row: ReviewRow) -> Result<Self, ApiError> {
        Ok(Self {
            author: row.author,
            outcome: row.outcome.parse().map_err(anyhow::Error::from)?,
            comment: row.comment,
            created_at: row.created_at,
        })
    }
}

async fn load(state: &Shared, id: &str) -> Result<Fix, ApiError> {
    let row: FixRow = sqlx::query_as(
        "SELECT project_id, title, summary, status, author, created_at, updated_at
         FROM fixes WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let problems = sqlx::query_scalar(
        "SELECT problem_id FROM fix_problems WHERE fix_id = ?1 ORDER BY problem_id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let tactics =
        sqlx::query_scalar("SELECT tactic FROM fix_tactics WHERE fix_id = ?1 ORDER BY tactic")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    let files = sqlx::query_scalar("SELECT path FROM fix_files WHERE fix_id = ?1 ORDER BY path")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let comparisons: Vec<ComparisonRow> = sqlx::query_as(
        "SELECT label, route, viewport, theme, before_artifact_id, after_artifact_id,
           diff_artifact_id, changed_ratio
         FROM fix_comparisons WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let steps: Vec<StepRow> = sqlx::query_as(
        "SELECT title, detail, verification, evidence_artifact_id, created_at
         FROM fix_steps WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let reviews: Vec<ReviewRow> = sqlx::query_as(
        "SELECT author, outcome, comment, created_at FROM fix_reviews
         WHERE fix_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let cited: Vec<(String, String)> = sqlx::query_as(
        "SELECT selection_id, role FROM fix_selections WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<String> = cited
        .iter()
        .map(|(selection, _)| selection.clone())
        .collect();
    let roles: std::collections::HashMap<String, String> = cited.into_iter().collect();
    let selections = crate::selections::load(&state.db, &ids)
        .await?
        .into_iter()
        .map(|selection| {
            let role = roles
                .get(&selection.id)
                .map_or(Ok(FixSelectionRole::Baseline), |role| role.parse())
                .map_err(anyhow::Error::from)?;
            Ok(FixSelection { role, selection })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Fix {
        id: id.to_owned(),
        project_id: row.project_id,
        title: row.title,
        summary: row.summary,
        status: row.status.parse().map_err(anyhow::Error::from)?,
        problems,
        tactics,
        files,
        comparisons: comparisons.into_iter().map(Comparison::from).collect(),
        selections,
        steps: steps.into_iter().map(Step::from).collect(),
        reviews: reviews
            .into_iter()
            .map(Review::try_from)
            .collect::<Result<_, _>>()?,
        author: row.author,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        Fix, FixSelectionRole, FixStatus, FixSummary, Problem, ProblemStatus, Selection,
    };

    async fn file_problem(port: u16, token: &str) -> String {
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(token),
            Some(r#"{"title":"Cramped","severity":"minor","origin":"user-reported"}"#),
        )
        .await;
        assert_eq!(status(&response), StatusCode::CREATED, "{response}");
        json::<Problem>(&response).id
    }

    fn new_fix(problem: &str, before: &str) -> String {
        format!(
            r#"{{"title":"Add spacing","summary":"8px grid","problems":["{problem}"],
                "tactics":["spacing"],"files":["src/ui.rs"],
                "comparisons":[{{"label":"home","before":"{before}","after":"{PROJECT_ARTIFACT}"}}],
                "steps":[{{"title":"measure","verification":"gap is 8px"}}]}}"#
        )
    }

    async fn problem_status(port: u16, token: &str, id: &str) -> ProblemStatus {
        json::<Problem>(
            &call(
                port,
                "GET",
                &format!("/v1/problems/{id}"),
                Some(token),
                None,
            )
            .await,
        )
        .status
    }

    #[tokio::test]
    async fn a_fix_moves_its_problems_through_review() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix(&problem, PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let fix: Fix = json(&created);
        assert_eq!(fix.status, FixStatus::PendingReview);
        assert_eq!(fix.comparisons.len(), 1);
        assert_eq!(fix.steps[0].step.title, "measure");
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::InProgress
        );

        let path = format!("/v1/fixes/{}", fix.id);
        let step = call(
            port,
            "POST",
            &format!("{path}/steps"),
            Some(&ana),
            Some(r#"{"title":"verify on device"}"#),
        )
        .await;
        assert_eq!(status(&step), StatusCode::CREATED, "{step}");

        let changes = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"changes-requested","comment":"more room"}"#),
        )
        .await;
        assert_eq!(json::<Fix>(&changes).status, FixStatus::ChangesRequested);
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::InProgress
        );

        let approved = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"approved"}"#),
        )
        .await;
        let approved: Fix = json(&approved);
        assert_eq!(approved.status, FixStatus::Approved);
        assert_eq!(approved.reviews.len(), 2);
        assert_eq!(approved.steps.len(), 2);
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::Resolved
        );

        let listed: Vec<FixSummary> =
            json(&call(port, "GET", "/v1/projects/demo/fixes", Some(&bruno), None).await);
        assert_eq!((listed.len(), listed[0].problems), (1, 1));
    }

    #[tokio::test]
    async fn fixes_are_checked_against_their_project_and_roles() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let foreign = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix(&problem, FOREIGN_ARTIFACT)),
        )
        .await;
        assert_eq!(
            status(&foreign),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{foreign}"
        );
        let unknown = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix("missing", PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(
            status(&unknown),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{unknown}"
        );
        let reader = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&bruno),
            Some(&new_fix(&problem, PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN, "{reader}");
        let fix: Fix = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/fixes",
                Some(&ana),
                Some(&new_fix(&problem, PROJECT_ARTIFACT)),
            )
            .await,
        );
        let path = format!("/v1/fixes/{}", fix.id);
        let hidden = call(port, "GET", &path, Some(&carla), None).await;
        assert_eq!(status(&hidden), StatusCode::NOT_FOUND, "{hidden}");
        let pending = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"pending-review"}"#),
        )
        .await;
        assert_eq!(
            status(&pending),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{pending}"
        );
    }

    /// The capture that holds the demo project's artifact.
    const PROJECT_CAPTURE: &str = "capture-shot";

    #[tokio::test]
    async fn a_fixs_comparisons_become_its_baseline_and_proof_and_it_can_bring_more() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let form = format!(
            r#"{{"title":"Add spacing","problems":["{problem}"],
                "comparisons":[{{"label":"home","before":"{PROJECT_ARTIFACT}","after":"{PROJECT_ARTIFACT}"}}],
                "proof":[{{"label":"Whole capture","items":[{{"capture":"{PROJECT_CAPTURE}","mark":"context"}}]}}]}}"#
        );
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&form),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let fix: Fix = json(&created);
        let cited: Vec<(FixSelectionRole, &str)> = fix
            .selections
            .iter()
            .map(|s| (s.role, s.selection.label.as_str()))
            .collect();
        assert_eq!(
            cited,
            [
                (FixSelectionRole::Baseline, "Before"),
                (FixSelectionRole::SolutionProof, "After"),
                (FixSelectionRole::SolutionProof, "Whole capture"),
            ]
        );
        assert_eq!(
            fix.comparisons.len(),
            1,
            "the comparison still pairs the files"
        );
    }

    #[tokio::test]
    async fn a_fix_cites_a_selection_changes_its_role_and_drops_it() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let fix: Fix = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/fixes",
                Some(&ana),
                Some(&format!(r#"{{"title":"Tidy","problems":["{problem}"]}}"#)),
            )
            .await,
        );
        assert_eq!(fix.selections.len(), 0);
        let selection: Selection = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/selections",
                Some(&ana),
                Some(&format!(
                    r#"{{"label":"Before the fix","items":[{{"capture":"{PROJECT_CAPTURE}","mark":"proof"}}]}}"#
                )),
            )
            .await,
        );
        let path = format!("/v1/fixes/{}/selections/{}", fix.id, selection.id);
        let as_role = |role: &str| format!(r#"{{"role":"{role}"}}"#);
        let cited: Fix =
            json(&call(port, "PUT", &path, Some(&ana), Some(&as_role("baseline"))).await);
        assert_eq!(cited.selections[0].role, FixSelectionRole::Baseline);
        let moved: Fix = json(
            &call(
                port,
                "PUT",
                &path,
                Some(&ana),
                Some(&as_role("solution-proof")),
            )
            .await,
        );
        assert_eq!(
            moved.selections.len(),
            1,
            "citing again changes the role, not the count"
        );
        assert_eq!(moved.selections[0].role, FixSelectionRole::SolutionProof);
        let reader = call(port, "PUT", &path, Some(&bruno), Some(&as_role("baseline"))).await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
        let dropped: Fix = json(&call(port, "DELETE", &path, Some(&ana), None).await);
        assert_eq!(dropped.selections.len(), 0);
        let elsewhere = format!("/v1/fixes/{}/selections/not-a-selection", fix.id);
        let missing = call(
            port,
            "PUT",
            &elsewhere,
            Some(&ana),
            Some(&as_role("baseline")),
        )
        .await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND);
        let outsider = call(port, "PUT", &path, Some(&carla), Some(&as_role("baseline"))).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
    }
}
