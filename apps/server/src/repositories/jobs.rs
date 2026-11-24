//! `capture_jobs` and `capture_job_captures`: requests to run a scenario, and what they made.

use crate::domain::ApiError;
use rupicola_api::{Job, JobCapture, JobStatus, TreeSpec};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

/// A `capture_jobs` row.
#[derive(sqlx::FromRow)]
struct JobRow {
    id: String,
    project_id: String,
    scenario: String,
    hash: String,
    fix_id: Option<String>,
    trees: String,
    status: String,
    cells_total: i64,
    cells_done: i64,
    runner: Option<String>,
    error: Option<String>,
    created_by: Option<String>,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}

/// `SELECT` of every job column, followed by `$tail`; sqlx takes only literal SQL.
macro_rules! select_jobs {
    ($tail:literal) => {
        concat!(
            "SELECT id, project_id, scenario, hash, fix_id, trees, status, cells_total,
                cells_done, runner, error, created_by, created_at, started_at, finished_at
             FROM capture_jobs ",
            $tail
        )
    };
}

/// A `capture_job_captures` row.
#[derive(sqlx::FromRow)]
struct CaptureRow {
    job_id: String,
    capture_id: String,
    tree: String,
    viewport: String,
    theme: String,
}

impl JobRow {
    fn into_job(self, captures: Vec<JobCapture>) -> Result<Job, ApiError> {
        let trees: Vec<TreeSpec> =
            serde_json::from_str(&self.trees).map_err(anyhow::Error::from)?;
        Ok(Job {
            id: self.id,
            project_id: self.project_id,
            scenario: self.scenario,
            hash: self.hash,
            fix: self.fix_id,
            trees,
            status: self.status.parse().map_err(anyhow::Error::from)?,
            cells_total: u32::try_from(self.cells_total).map_err(anyhow::Error::from)?,
            cells_done: u32::try_from(self.cells_done).map_err(anyhow::Error::from)?,
            runner: self.runner,
            error: self.error,
            captures,
            created_by: self.created_by,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
        })
    }
}

/// What a new job holds.
pub struct NewRow<'a> {
    pub id: &'a str,
    pub project: &'a str,
    pub scenario: &'a str,
    pub hash: &'a str,
    pub fix: Option<&'a str>,
    pub trees: &'a [TreeSpec],
    pub cells_total: u32,
    pub created_by: Option<&'a str>,
    pub now: i64,
}

/// Queues a job.
pub async fn insert(db: &SqlitePool, job: &NewRow<'_>) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO capture_jobs (id, project_id, scenario, hash, fix_id, trees, status,
           cells_total, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(job.id)
    .bind(job.project)
    .bind(job.scenario)
    .bind(job.hash)
    .bind(job.fix)
    .bind(serde_json::to_string(job.trees).map_err(anyhow::Error::from)?)
    .bind(JobStatus::Queued.as_str())
    .bind(i64::from(job.cells_total))
    .bind(job.created_by)
    .bind(job.now)
    .execute(db)
    .await?;
    Ok(())
}

async fn captures_of(
    db: &SqlitePool,
    ids: &[String],
) -> Result<HashMap<String, Vec<JobCapture>>, ApiError> {
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT job_id, capture_id, tree, viewport, theme FROM capture_job_captures WHERE job_id",
    );
    super::push_in(&mut qb, ids);
    qb.push(" ORDER BY job_id, position");
    let rows: Vec<CaptureRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_job: HashMap<String, Vec<JobCapture>> = HashMap::new();
    for row in rows {
        by_job.entry(row.job_id).or_default().push(JobCapture {
            capture: row.capture_id,
            tree: row.tree,
            viewport: row.viewport,
            theme: row.theme,
        });
    }
    Ok(by_job)
}

/// One job, with what it made.
pub async fn get(db: &SqlitePool, id: &str) -> Result<Option<Job>, ApiError> {
    let row: Option<JobRow> = sqlx::query_as(select_jobs!("WHERE id = ?1"))
        .bind(id)
        .fetch_optional(db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let captures = captures_of(db, std::slice::from_ref(&row.id))
        .await?
        .remove(&row.id)
        .unwrap_or_default();
    row.into_job(captures).map(Some)
}

/// The project a job belongs to.
pub async fn project_of(db: &SqlitePool, id: &str) -> Result<Option<String>, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT project_id FROM capture_jobs WHERE id = ?1")
            .bind(id)
            .fetch_optional(db)
            .await?,
    )
}

/// The project's jobs, newest first, optionally only the ones a runner still has to do or is
/// doing.
pub async fn list(db: &SqlitePool, project: &str, open_only: bool) -> Result<Vec<Job>, ApiError> {
    let rows: Vec<JobRow> = sqlx::query_as(select_jobs!(
        "WHERE project_id = ?1 AND (?2 = 0 OR status IN ('queued', 'running'))
         ORDER BY created_at DESC, id DESC LIMIT ?3"
    ))
    .bind(project)
    .bind(i64::from(open_only))
    .bind(super::MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let mut captures = if ids.is_empty() {
        HashMap::new()
    } else {
        captures_of(db, &ids).await?
    };
    rows.into_iter()
        .map(|row| {
            let made = captures.remove(&row.id).unwrap_or_default();
            row.into_job(made)
        })
        .collect()
}

/// Gives the oldest job a runner can take to `key`: a queued one, or a running one whose lease
/// has run out. A job taken over starts again from no captures. Returns its id.
pub async fn claim(
    db: &SqlitePool,
    project: &str,
    key: &str,
    runner: &str,
    now: i64,
    lease_ms: i64,
) -> Result<Option<String>, ApiError> {
    Ok(sqlx::query_scalar(
        "UPDATE capture_jobs
         SET status = 'running', runner = ?3, runner_key = ?2, lease_expires_at = ?4 + ?5,
             started_at = ?4, cells_done = 0, error = NULL
         WHERE id = (
           SELECT id FROM capture_jobs
           WHERE project_id = ?1
             AND (status = 'queued' OR (status = 'running' AND lease_expires_at < ?4))
           ORDER BY created_at, id LIMIT 1)
         RETURNING id",
    )
    .bind(project)
    .bind(key)
    .bind(runner)
    .bind(now)
    .bind(lease_ms)
    .fetch_optional(db)
    .await?)
}

/// Records progress and extends the lease. False when `key` does not hold the job.
pub async fn progress(
    db: &SqlitePool,
    id: &str,
    key: &str,
    cells_done: u32,
    now: i64,
    lease_ms: i64,
) -> Result<bool, ApiError> {
    Ok(sqlx::query(
        "UPDATE capture_jobs SET cells_done = ?3, lease_expires_at = ?4 + ?5
         WHERE id = ?1 AND runner_key = ?2 AND status = 'running'",
    )
    .bind(id)
    .bind(key)
    .bind(i64::from(cells_done))
    .bind(now)
    .bind(lease_ms)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

/// Marks the job done with what it made. False when `key` does not hold it.
pub async fn complete(
    tx: &mut SqliteConnection,
    id: &str,
    key: &str,
    captures: &[JobCapture],
    now: i64,
) -> Result<bool, ApiError> {
    let held = sqlx::query(
        "UPDATE capture_jobs SET status = 'done', cells_done = cells_total, finished_at = ?3,
           lease_expires_at = NULL
         WHERE id = ?1 AND runner_key = ?2 AND status = 'running'",
    )
    .bind(id)
    .bind(key)
    .bind(now)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if !held {
        return Ok(false);
    }
    for (position, capture) in captures.iter().enumerate() {
        sqlx::query(
            "INSERT INTO capture_job_captures (job_id, position, capture_id, tree, viewport, theme)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .bind(&capture.capture)
        .bind(&capture.tree)
        .bind(&capture.viewport)
        .bind(&capture.theme)
        .execute(&mut *tx)
        .await?;
    }
    Ok(true)
}

/// Marks the job failed. False when `key` does not hold it.
pub async fn fail(
    db: &SqlitePool,
    id: &str,
    key: &str,
    error: &str,
    now: i64,
) -> Result<bool, ApiError> {
    Ok(sqlx::query(
        "UPDATE capture_jobs SET status = 'failed', error = ?3, finished_at = ?4,
           lease_expires_at = NULL
         WHERE id = ?1 AND runner_key = ?2 AND status = 'running'",
    )
    .bind(id)
    .bind(key)
    .bind(error)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

/// Cancels a job that has not finished. False when it already had.
pub async fn cancel(db: &SqlitePool, id: &str, now: i64) -> Result<bool, ApiError> {
    Ok(sqlx::query(
        "UPDATE capture_jobs SET status = 'cancelled', finished_at = ?2, lease_expires_at = NULL
         WHERE id = ?1 AND status IN ('queued', 'running')",
    )
    .bind(id)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::open_memory;

    const PROJECT: &str = "demo";
    const LEASE_MS: i64 = 1_000;
    const START_MS: i64 = 10_000;

    async fn queued(db: &SqlitePool, id: &str, now: i64) {
        insert(
            db,
            &NewRow {
                id,
                project: PROJECT,
                scenario: "home",
                hash: "h",
                fix: None,
                trees: &[TreeSpec::default()],
                cells_total: 4,
                created_by: None,
                now,
            },
        )
        .await
        .unwrap();
    }

    async fn project_db() -> SqlitePool {
        let db = open_memory().await;
        sqlx::query("INSERT INTO projects (id, label, created_at) VALUES (?1, ?1, 0)")
            .bind(PROJECT)
            .execute(&db)
            .await
            .unwrap();
        db
    }

    #[tokio::test]
    async fn a_job_is_taken_by_one_runner_until_its_lease_runs_out() {
        let db = project_db().await;
        queued(&db, "old", START_MS).await;
        queued(&db, "new", START_MS + 1).await;

        let first = claim(&db, PROJECT, "a", "ana", START_MS, LEASE_MS)
            .await
            .unwrap();
        assert_eq!(first.as_deref(), Some("old"), "the oldest job goes first");
        let second = claim(&db, PROJECT, "b", "bia", START_MS, LEASE_MS)
            .await
            .unwrap();
        assert_eq!(second.as_deref(), Some("new"));
        assert_eq!(
            claim(&db, PROJECT, "c", "cid", START_MS, LEASE_MS)
                .await
                .unwrap(),
            None
        );

        assert!(
            progress(&db, "old", "a", 3, START_MS, LEASE_MS)
                .await
                .unwrap()
        );
        assert!(
            !progress(&db, "old", "b", 4, START_MS, LEASE_MS)
                .await
                .unwrap()
        );

        // Only "old" kept reporting; "new" was abandoned and is taken over from the start.
        let later = START_MS + LEASE_MS + 1;
        assert!(
            progress(&db, "old", "a", 3, later - 1, LEASE_MS)
                .await
                .unwrap()
        );
        let taken = claim(&db, PROJECT, "c", "cid", later, LEASE_MS)
            .await
            .unwrap();
        assert_eq!(taken.as_deref(), Some("new"));
        let job = get(&db, "new").await.unwrap().unwrap();
        assert_eq!((job.runner.as_deref(), job.cells_done), (Some("cid"), 0));
        assert!(!progress(&db, "new", "b", 1, later, LEASE_MS).await.unwrap());
    }

    #[tokio::test]
    async fn a_finished_job_cannot_be_reported_on_or_cancelled() {
        let db = project_db().await;
        queued(&db, "job", START_MS).await;
        claim(&db, PROJECT, "a", "ana", START_MS, LEASE_MS)
            .await
            .unwrap();
        assert!(fail(&db, "job", "a", "no browser", START_MS).await.unwrap());
        assert!(
            !progress(&db, "job", "a", 1, START_MS, LEASE_MS)
                .await
                .unwrap()
        );
        assert!(!cancel(&db, "job", START_MS).await.unwrap());
        let job = get(&db, "job").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.as_deref(), Some("no browser"));
    }
}
