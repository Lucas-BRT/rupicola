//! SQLite queries for `fixes`, `fix_problems`, `fix_tactics`, `fix_files`,
//! `fix_comparisons`, `fix_selections`, `fix_steps` and `fix_reviews`.

mod batch;

pub use batch::list_details;

use crate::{domain::ApiError, repositories::selections};
use rupicola_api::{
    Comparison, Fix, FixSelection, FixSelectionRole, FixStatus, FixSummary, NewFix, NewStep,
    ProblemStatus, Review, Step,
};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::{BTreeSet, HashMap};

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
pub(super) struct FixRow {
    project_id: String,
    title: String,
    summary: String,
    git_ref: Option<String>,
    status: String,
    mismatch_override_reason: Option<String>,
    author: Option<String>,
    created_at: i64,
    updated_at: i64,
}

/// A `fix_comparisons` row.
#[derive(sqlx::FromRow)]
pub(super) struct ComparisonRow {
    label: String,
    route: String,
    viewport: String,
    theme: String,
    before_artifact_id: String,
    after_artifact_id: String,
    diff_artifact_id: Option<String>,
    changed_ratio: Option<f64>,
    view: String,
    base_revision: String,
    revision: String,
    applied_fixes: String,
}

impl TryFrom<ComparisonRow> for Comparison {
    type Error = ApiError;

    fn try_from(row: ComparisonRow) -> Result<Self, ApiError> {
        Ok(Self {
            label: row.label,
            view: row.view.parse().map_err(anyhow::Error::from)?,
            base_revision: row.base_revision,
            revision: row.revision,
            applied_fixes: serde_json::from_str(&row.applied_fixes).map_err(anyhow::Error::from)?,
            stale: false,
            route: row.route,
            viewport: row.viewport,
            theme: row.theme,
            before: row.before_artifact_id,
            after: row.after_artifact_id,
            diff: row.diff_artifact_id,
            changed_ratio: row.changed_ratio,
        })
    }
}

/// Ids of the project's approved fixes, which an integrated comparison is taken against.
pub(super) async fn approved_ids(
    db: &SqlitePool,
    project: &str,
) -> Result<BTreeSet<String>, ApiError> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM fixes WHERE project_id = ?1 AND status = ?2")
            .bind(project)
            .bind(FixStatus::Approved.as_str())
            .fetch_all(db)
            .await?;
    Ok(ids.into_iter().collect())
}

/// Converts comparison rows and flags the integrated ones the approved fixes have outdated.
pub(super) fn comparisons_of(
    rows: Vec<ComparisonRow>,
    fix: &str,
    approved: &BTreeSet<String>,
) -> Result<Vec<Comparison>, ApiError> {
    rows.into_iter()
        .map(|row| {
            let mut comparison = Comparison::try_from(row)?;
            comparison.stale = comparison.is_stale(fix, approved);
            Ok(comparison)
        })
        .collect()
}

/// A `fix_steps` row.
#[derive(sqlx::FromRow)]
pub(super) struct StepRow {
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
pub(super) struct ReviewRow {
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

/// The project that owns `fix`, if it exists.
pub async fn project_of(db: &SqlitePool, fix: &str) -> Result<Option<String>, ApiError> {
    let project = sqlx::query_scalar("SELECT project_id FROM fixes WHERE id = ?1")
        .bind(fix)
        .fetch_optional(db)
        .await?;
    Ok(project)
}

/// Whether `problem` belongs to `project`.
pub async fn problem_belongs(
    db: &SqlitePool,
    problem: &str,
    project: &str,
) -> Result<bool, ApiError> {
    let belongs: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM problems WHERE id = ?1 AND project_id = ?2")
            .bind(problem)
            .bind(project)
            .fetch_optional(db)
            .await?;
    Ok(belongs.is_some())
}

/// The capture holding `artifact` in `project`, if any.
pub async fn capture_of_artifact(
    tx: &mut SqliteConnection,
    project: &str,
    artifact: &str,
) -> Result<Option<String>, ApiError> {
    let capture = sqlx::query_scalar(
        "SELECT a.capture_id FROM artifacts a JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    Ok(capture)
}

/// Capture & plan metadata for one artifact, used by the anti-hallucination parity guard.
#[derive(sqlx::FromRow)]
pub struct ArtifactPlanMeta {
    pub capture_id: String,
    pub owner_id: String,
    pub plan_json: Option<String>,
    pub plan_digest: Option<String>,
}

/// Loads the owning capture, checkpoint/clip `owner_id`, and frozen plan metadata for `artifact`.
pub async fn artifact_plan_meta(
    db: &SqlitePool,
    project: &str,
    artifact: &str,
) -> Result<Option<ArtifactPlanMeta>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT a.capture_id, a.owner_id, c.plan_json, c.plan_digest
         FROM artifacts a
         JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(db)
    .await?)
}

/// Returns `(capture_id, plan_digest)` for every evidence selection anchor cited by `problems`.
pub async fn problem_evidence_plans(
    db: &SqlitePool,
    problems: &[&str],
) -> Result<Vec<(String, Option<String>)>, ApiError> {
    if problems.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = problems.iter().map(|s| (*s).to_owned()).collect();
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT DISTINCT si.capture_id, c.plan_digest
         FROM problem_selections ps
         JOIN selection_items si ON si.selection_id = ps.selection_id
         JOIN captures c ON c.id = si.capture_id
         WHERE ps.problem_id",
    );
    super::push_in(&mut qb, &ids);
    Ok(qb.build_query_as().fetch_all(db).await?)
}

/// Inserts a fix header row inside `tx`.
pub async fn insert(
    tx: &mut SqliteConnection,
    id: &str,
    project: &str,
    form: &NewFix,
    author: Option<&str>,
    now: i64,
) -> Result<(), ApiError> {
    let override_reason = form
        .mismatch_override_reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    sqlx::query(
        "INSERT INTO fixes (id, project_id, title, summary, git_ref, status, mismatch_override_reason, author, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending-review', ?6, ?7, ?8, ?8)",
    )
    .bind(id)
    .bind(project)
    .bind(form.title.trim())
    .bind(&form.summary)
    .bind(form.git_ref.as_deref().map(str::trim))
    .bind(override_reason)
    .bind(author)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Links `fix` to `problem` inside `tx`.
pub async fn link_problem(
    tx: &mut SqliteConnection,
    fix: &str,
    problem: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO fix_problems (fix_id, problem_id) VALUES (?1, ?2)")
        .bind(fix)
        .bind(problem)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Updates a problem's status and timestamp inside `tx`.
pub async fn set_problem_status(
    tx: &mut SqliteConnection,
    problem: &str,
    status: ProblemStatus,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE problems SET status = ?2, updated_at = ?3 WHERE id = ?1")
        .bind(problem)
        .bind(status.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Inserts tactics, files, comparisons and steps of a new fix inside `tx`.
pub async fn insert_details(
    tx: &mut SqliteConnection,
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
               before_artifact_id, after_artifact_id, diff_artifact_id, changed_ratio,
               view, base_revision, revision, applied_fixes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
        .bind(comparison.view.as_str())
        .bind(&comparison.base_revision)
        .bind(&comparison.revision)
        .bind(serde_json::to_string(&comparison.applied_fixes).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await?;
    }
    for step in &form.steps {
        insert_step(tx, id, step, now).await?;
    }
    Ok(())
}

/// Inserts one step on `fix` inside `tx`.
pub async fn insert_step(
    tx: &mut SqliteConnection,
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
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Updates `fixes.updated_at` inside `tx`.
pub async fn touch(tx: &mut SqliteConnection, fix: &str, now: i64) -> Result<(), ApiError> {
    sqlx::query("UPDATE fixes SET updated_at = ?2 WHERE id = ?1")
        .bind(fix)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Makes `fix` cite `selection` as `role`, at `position`.
pub async fn cite(
    tx: &mut SqliteConnection,
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

/// Next selection position for `fix` inside `tx`.
pub async fn next_selection_position(
    tx: &mut SqliteConnection,
    fix: &str,
) -> Result<i64, ApiError> {
    let next = sqlx::query_scalar(
        "SELECT coalesce(max(position) + 1, 0) FROM fix_selections WHERE fix_id = ?1",
    )
    .bind(fix)
    .fetch_one(&mut *tx)
    .await?;
    Ok(next)
}

/// Removes `selection` from `fix`'s cited selections.
pub async fn uncite(db: &SqlitePool, fix: &str, selection: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM fix_selections WHERE fix_id = ?1 AND selection_id = ?2")
        .bind(fix)
        .bind(selection)
        .execute(db)
        .await?;
    Ok(())
}

/// Lists every fix summary of `project`, newest first.
pub async fn list(db: &SqlitePool, project: &str) -> Result<Vec<FixSummary>, ApiError> {
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT f.id, f.title, f.status,
           (SELECT COUNT(*) FROM fix_problems p WHERE p.fix_id = f.id) AS problems, f.updated_at
         FROM fixes f WHERE f.project_id = ?1 ORDER BY f.updated_at DESC, f.id DESC LIMIT ?2",
    )
    .bind(project)
    .bind(super::MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(FixSummary::try_from).collect()
}

/// Records a review, updates the fix status and returns the linked problem ids inside `tx`.
pub async fn apply_review(
    tx: &mut SqliteConnection,
    fix: &str,
    author: Option<&str>,
    outcome: FixStatus,
    comment: &str,
    now: i64,
) -> Result<Vec<String>, ApiError> {
    sqlx::query(
        "INSERT INTO fix_reviews (id, fix_id, author, outcome, comment, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(fix)
    .bind(author)
    .bind(outcome.as_str())
    .bind(comment)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE fixes SET status = ?2, updated_at = ?3 WHERE id = ?1")
        .bind(fix)
        .bind(outcome.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let problems: Vec<String> =
        sqlx::query_scalar("SELECT problem_id FROM fix_problems WHERE fix_id = ?1")
            .bind(fix)
            .fetch_all(&mut *tx)
            .await?;
    Ok(problems)
}

/// Loads one fix with its problems, tactics, files, comparisons, selections, steps and reviews.
pub async fn load(db: &SqlitePool, id: &str) -> Result<Fix, ApiError> {
    let row: FixRow = sqlx::query_as(
        "SELECT project_id, title, summary, git_ref, status, mismatch_override_reason, author, created_at, updated_at
         FROM fixes WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let problems = sqlx::query_scalar(
        "SELECT problem_id FROM fix_problems WHERE fix_id = ?1 ORDER BY problem_id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let tactics =
        sqlx::query_scalar("SELECT tactic FROM fix_tactics WHERE fix_id = ?1 ORDER BY tactic")
            .bind(id)
            .fetch_all(db)
            .await?;
    let files = sqlx::query_scalar("SELECT path FROM fix_files WHERE fix_id = ?1 ORDER BY path")
        .bind(id)
        .fetch_all(db)
        .await?;
    let comparisons: Vec<ComparisonRow> = sqlx::query_as(
        "SELECT label, route, viewport, theme, before_artifact_id, after_artifact_id,
           diff_artifact_id, changed_ratio, view, base_revision, revision, applied_fixes
         FROM fix_comparisons WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let approved = approved_ids(db, &row.project_id).await?;
    let steps: Vec<StepRow> = sqlx::query_as(
        "SELECT title, detail, verification, evidence_artifact_id, created_at
         FROM fix_steps WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let reviews: Vec<ReviewRow> = sqlx::query_as(
        "SELECT author, outcome, comment, created_at FROM fix_reviews
         WHERE fix_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let cited: Vec<(String, String)> = sqlx::query_as(
        "SELECT selection_id, role FROM fix_selections WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let ids: Vec<String> = cited
        .iter()
        .map(|(selection, _)| selection.clone())
        .collect();
    let roles: HashMap<String, String> = cited.into_iter().collect();
    let selections = selections::load(db, &ids)
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
        git_ref: row.git_ref,
        status: row.status.parse().map_err(anyhow::Error::from)?,
        problems,
        tactics,
        files,
        comparisons: comparisons_of(comparisons, id, &approved)?,
        selections,
        steps: steps.into_iter().map(Step::from).collect(),
        reviews: reviews
            .into_iter()
            .map(Review::try_from)
            .collect::<Result<_, _>>()?,
        mismatch_override_reason: row.mismatch_override_reason,
        author: row.author,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Approved fixes of `project` that carry a `git_ref`, ordered deterministically for merging.
pub async fn approved_with_refs(
    db: &SqlitePool,
    project: &str,
) -> Result<Vec<(String, String)>, ApiError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, git_ref FROM fixes
         WHERE project_id = ?1 AND status = 'approved'
           AND git_ref IS NOT NULL AND trim(git_ref) != ''
         ORDER BY created_at, id",
    )
    .bind(project)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Pending fixes of `project` with a `git_ref`, excluding `exclude_fix`.
pub async fn pending_with_refs(
    db: &SqlitePool,
    project: &str,
    exclude_fix: &str,
) -> Result<Vec<(String, String)>, ApiError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, git_ref FROM fixes
         WHERE project_id = ?1 AND status = 'pending-review' AND id != ?2
           AND git_ref IS NOT NULL AND trim(git_ref) != ''
         ORDER BY created_at, id",
    )
    .bind(project)
    .bind(exclude_fix)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Replaces comparisons of `fix_id` for `views` with `comparisons`, preserving other views.
pub async fn replace_comparisons_for_views(
    tx: &mut SqliteConnection,
    fix_id: &str,
    views: &[rupicola_api::ComparisonView],
    comparisons: &[Comparison],
) -> Result<(), ApiError> {
    if views.is_empty() {
        return Ok(());
    }
    let mut qb = sqlx::QueryBuilder::new("DELETE FROM fix_comparisons WHERE fix_id = ");
    qb.push_bind(fix_id);
    qb.push(" AND view");
    let view_strs: Vec<String> = views.iter().map(|v| v.as_str().to_owned()).collect();
    super::push_in(&mut qb, &view_strs);
    qb.build().execute(&mut *tx).await?;

    let base_pos: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(position) + 1, 0) FROM fix_comparisons WHERE fix_id = ?1",
    )
    .bind(fix_id)
    .fetch_one(&mut *tx)
    .await?;

    for (offset, comparison) in comparisons.iter().enumerate() {
        let pos = base_pos + i64::try_from(offset).map_err(anyhow::Error::from)?;
        sqlx::query(
            "INSERT INTO fix_comparisons (id, fix_id, position, label, route, viewport, theme,
               before_artifact_id, after_artifact_id, diff_artifact_id, changed_ratio,
               view, base_revision, revision, applied_fixes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(fix_id)
        .bind(pos)
        .bind(&comparison.label)
        .bind(&comparison.route)
        .bind(&comparison.viewport)
        .bind(&comparison.theme)
        .bind(&comparison.before)
        .bind(&comparison.after)
        .bind(&comparison.diff)
        .bind(comparison.changed_ratio)
        .bind(comparison.view.as_str())
        .bind(&comparison.base_revision)
        .bind(&comparison.revision)
        .bind(serde_json::to_string(&comparison.applied_fixes).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Removes previous auto-generated `Before`/`After` citations on `fix_id` and cites `new_links`.
pub async fn replace_auto_selections(
    tx: &mut SqliteConnection,
    project: &str,
    fix_id: &str,
    before_label: &str,
    after_label: &str,
    new_links: &[(FixSelectionRole, String)],
) -> Result<(), ApiError> {
    sqlx::query(
        "DELETE FROM fix_selections
         WHERE fix_id = ?1 AND selection_id IN (
             SELECT id FROM selections WHERE project_id = ?2 AND label IN (?3, ?4)
         )",
    )
    .bind(fix_id)
    .bind(project)
    .bind(before_label)
    .bind(after_label)
    .execute(&mut *tx)
    .await?;

    let base_pos = next_selection_position(tx, fix_id).await?;
    for (offset, (role, selection_id)) in new_links.iter().enumerate() {
        let pos = base_pos + i64::try_from(offset).map_err(anyhow::Error::from)?;
        cite(tx, project, fix_id, selection_id, *role, pos).await?;
    }
    Ok(())
}
