//! SQLite queries for `fixes`, `fix_problems`, `fix_tactics`, `fix_files`,
//! `fix_comparisons`, `fix_selections`, `fix_steps` and `fix_reviews`.

use crate::{domain::ApiError, repositories::selections};
use rupicola_api::{
    Comparison, Fix, FixSelection, FixSelectionRole, FixStatus, FixSummary, NewFix, NewStep,
    ProblemStatus, Review, Step,
};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

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

/// Inserts a fix header row inside `tx`.
pub async fn insert(
    tx: &mut SqliteConnection,
    id: &str,
    project: &str,
    form: &NewFix,
    author: Option<&str>,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO fixes (id, project_id, title, summary, status, author, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'pending-review', ?5, ?6, ?6)",
    )
    .bind(id)
    .bind(project)
    .bind(form.title.trim())
    .bind(&form.summary)
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

#[derive(sqlx::FromRow)]
struct KeyedFixRow {
    id: String,
    #[sqlx(flatten)]
    row: FixRow,
}

#[derive(sqlx::FromRow)]
struct KeyedTextRow {
    fix_id: String,
    value: String,
}

#[derive(sqlx::FromRow)]
struct KeyedComparisonRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: ComparisonRow,
}

#[derive(sqlx::FromRow)]
struct KeyedStepRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: StepRow,
}

#[derive(sqlx::FromRow)]
struct KeyedReviewRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: ReviewRow,
}

#[derive(sqlx::FromRow)]
struct KeyedFixSelectionRow {
    fix_id: String,
    selection_id: String,
    role: String,
}

async fn batch_texts(
    db: &SqlitePool,
    prefix: &str,
    order: &str,
    ids: &[String],
) -> Result<HashMap<String, Vec<String>>, ApiError> {
    let mut qb = sqlx::QueryBuilder::new(prefix);
    super::push_in(&mut qb, ids);
    qb.push(order);
    let rows: Vec<KeyedTextRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_id: HashMap<String, Vec<String>> = HashMap::new();
    for item in rows {
        by_id.entry(item.fix_id).or_default().push(item.value);
    }
    Ok(by_id)
}

async fn batch_fix_selections(
    db: &SqlitePool,
    ids: &[String],
) -> Result<HashMap<String, Vec<FixSelection>>, ApiError> {
    use std::collections::HashSet;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, selection_id, role FROM fix_selections WHERE fix_id",
    );
    super::push_in(&mut qb, ids);
    qb.push(" ORDER BY fix_id, position");
    let sel_rows: Vec<KeyedFixSelectionRow> = qb.build_query_as().fetch_all(db).await?;
    let mut links_by_id: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut unique_sel_ids = HashSet::new();
    let mut all_sel_ids = Vec::new();
    for item in sel_rows {
        if unique_sel_ids.insert(item.selection_id.clone()) {
            all_sel_ids.push(item.selection_id.clone());
        }
        links_by_id
            .entry(item.fix_id)
            .or_default()
            .push((item.selection_id, item.role));
    }
    let selections_by_id: HashMap<String, rupicola_api::Selection> = if all_sel_ids.is_empty() {
        HashMap::new()
    } else {
        selections::load(db, &all_sel_ids)
            .await?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };
    let mut out = HashMap::with_capacity(links_by_id.len());
    for (fix_id, links) in links_by_id {
        let mut list = Vec::with_capacity(links.len());
        for (sid, role_str) in links {
            if let Some(selection) = selections_by_id.get(&sid) {
                let role = role_str.parse().map_err(anyhow::Error::from)?;
                list.push(FixSelection {
                    role,
                    selection: selection.clone(),
                });
            }
        }
        out.insert(fix_id, list);
    }
    Ok(out)
}

async fn batch_comparisons(
    db: &SqlitePool,
    ids: &[String],
) -> Result<HashMap<String, Vec<ComparisonRow>>, ApiError> {
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, label, route, viewport, theme, before_artifact_id, after_artifact_id,
                diff_artifact_id, changed_ratio
         FROM fix_comparisons WHERE fix_id",
    );
    super::push_in(&mut qb, ids);
    qb.push(" ORDER BY fix_id, position");
    let rows: Vec<KeyedComparisonRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_id: HashMap<String, Vec<ComparisonRow>> = HashMap::new();
    for item in rows {
        by_id.entry(item.fix_id).or_default().push(item.row);
    }
    Ok(by_id)
}

/// Lists every fix of `project` in full, newest first, in bounded batch queries.
pub async fn list_details(db: &SqlitePool, project: &str) -> Result<Vec<Fix>, ApiError> {
    let fix_rows: Vec<KeyedFixRow> = sqlx::query_as(
        "SELECT id, project_id, title, summary, status, author, created_at, updated_at
         FROM fixes WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC LIMIT ?2",
    )
    .bind(project)
    .bind(super::MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    if fix_rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = fix_rows.iter().map(|r| r.id.clone()).collect();

    let mut problems_by_id = batch_texts(
        db,
        "SELECT fix_id, problem_id AS value FROM fix_problems WHERE fix_id",
        " ORDER BY fix_id, problem_id",
        &ids,
    )
    .await?;
    let mut tactics_by_id = batch_texts(
        db,
        "SELECT fix_id, tactic AS value FROM fix_tactics WHERE fix_id",
        " ORDER BY fix_id, tactic",
        &ids,
    )
    .await?;
    let mut files_by_id = batch_texts(
        db,
        "SELECT fix_id, path AS value FROM fix_files WHERE fix_id",
        " ORDER BY fix_id, path",
        &ids,
    )
    .await?;

    let mut comparisons_by_id = batch_comparisons(db, &ids).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, title, detail, verification, evidence_artifact_id, created_at
         FROM fix_steps WHERE fix_id",
    );
    super::push_in(&mut qb, &ids);
    qb.push(" ORDER BY fix_id, position");
    let step_rows: Vec<KeyedStepRow> = qb.build_query_as().fetch_all(db).await?;
    let mut steps_by_id: HashMap<String, Vec<StepRow>> = HashMap::new();
    for item in step_rows {
        steps_by_id.entry(item.fix_id).or_default().push(item.row);
    }

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, author, outcome, comment, created_at FROM fix_reviews WHERE fix_id",
    );
    super::push_in(&mut qb, &ids);
    qb.push(" ORDER BY fix_id, created_at, id");
    let review_rows: Vec<KeyedReviewRow> = qb.build_query_as().fetch_all(db).await?;
    let mut reviews_by_id: HashMap<String, Vec<ReviewRow>> = HashMap::new();
    for item in review_rows {
        reviews_by_id.entry(item.fix_id).or_default().push(item.row);
    }

    let mut selections_by_fix = batch_fix_selections(db, &ids).await?;

    let mut detailed = Vec::with_capacity(fix_rows.len());
    for keyed in fix_rows {
        let id = keyed.id;
        let row = keyed.row;
        let fix_selections = selections_by_fix.remove(&id).unwrap_or_default();
        let reviews = reviews_by_id
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .map(Review::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        detailed.push(Fix {
            id: id.clone(),
            project_id: row.project_id,
            title: row.title,
            summary: row.summary,
            status: row.status.parse().map_err(anyhow::Error::from)?,
            problems: problems_by_id.remove(&id).unwrap_or_default(),
            tactics: tactics_by_id.remove(&id).unwrap_or_default(),
            files: files_by_id.remove(&id).unwrap_or_default(),
            comparisons: comparisons_by_id
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(Comparison::from)
                .collect(),
            selections: fix_selections,
            steps: steps_by_id
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(Step::from)
                .collect(),
            reviews,
            author: row.author,
            created_at: row.created_at,
            updated_at: row.updated_at,
        });
    }
    Ok(detailed)
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
        "SELECT project_id, title, summary, status, author, created_at, updated_at
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
           diff_artifact_id, changed_ratio
         FROM fix_comparisons WHERE fix_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
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
