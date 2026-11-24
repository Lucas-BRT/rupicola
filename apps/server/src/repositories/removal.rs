//! SQLite queries for capture citations, tombstones and project deletion policies.

use crate::{
    domain::{
        ApiError,
        removal::{DEFAULT_POLICY, POLICY_KEY},
    },
    repositories::corrupt,
};
use rupicola_api::{Citation, CitationKind, DeletionPolicy};
use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

/// The project's policy as it stands, before the server's ceiling.
pub async fn project_policy(
    db: impl SqliteExecutor<'_>,
    project: &str,
) -> Result<DeletionPolicy, ApiError> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT value FROM project_settings WHERE project_id = ?1 AND key = ?2")
            .bind(project)
            .bind(POLICY_KEY)
            .fetch_optional(db)
            .await?;
    match stored {
        None => Ok(DEFAULT_POLICY),
        Some(raw) => {
            let value: String = serde_json::from_str(&raw).map_err(corrupt)?;
            value.parse().map_err(corrupt)
        }
    }
}

/// Persists the project's cited-deletion policy inside `tx`.
pub async fn set_project_policy(
    tx: &mut SqliteConnection,
    project: &str,
    policy: DeletionPolicy,
    actor: &str,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO project_settings (project_id, key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value,
           generation = generation + 1, updated_by = excluded.updated_by,
           updated_at = excluded.updated_at",
    )
    .bind(project)
    .bind(POLICY_KEY)
    .bind(serde_json::to_string(policy.as_str()).map_err(anyhow::Error::from)?)
    .bind(actor)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The project of a live capture, if it exists and has not been removed.
pub async fn live_capture_project(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> Result<Option<String>, ApiError> {
    let project =
        sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1 AND deleted_at IS NULL")
            .bind(capture)
            .fetch_optional(db)
            .await?;
    Ok(project)
}

/// Everything that cites `capture`, each once, in a stable order.
pub async fn cited_by(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> Result<Vec<Citation>, ApiError> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "WITH files AS (SELECT id FROM artifacts WHERE capture_id = ?1),
              chosen AS (SELECT DISTINCT selection_id FROM selection_items WHERE capture_id = ?1)
         SELECT 'problem', p.id, p.title, p.status FROM problem_selections ps
           JOIN chosen c ON c.selection_id = ps.selection_id JOIN problems p ON p.id = ps.problem_id
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_selections fs
           JOIN chosen c ON c.selection_id = fs.selection_id JOIN fixes f ON f.id = fs.fix_id
         UNION
         SELECT 'selection', s.id, s.label, NULL FROM selections s JOIN chosen c ON c.selection_id = s.id
           WHERE NOT EXISTS (SELECT 1 FROM problem_selections WHERE selection_id = s.id)
             AND NOT EXISTS (SELECT 1 FROM fix_selections WHERE selection_id = s.id)
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_comparisons k JOIN fixes f ON f.id = k.fix_id
           WHERE k.before_artifact_id IN files OR k.after_artifact_id IN files
              OR k.diff_artifact_id IN files
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_steps t JOIN fixes f ON f.id = t.fix_id
           WHERE t.evidence_artifact_id IN files
         UNION
         SELECT 'design', d.id, d.title, d.status FROM design_entries d WHERE d.evidence IN files
         UNION
         SELECT 'finding', n.id, n.note, NULL FROM capture_findings n
           WHERE n.evidence_artifact_id IN files AND n.capture_id <> ?1",
    )
    .bind(capture)
    .fetch_all(db)
    .await?;
    let mut citations: Vec<Citation> = rows
        .into_iter()
        .map(|(kind, id, title, status)| {
            Ok(Citation {
                kind: kind.parse::<CitationKind>().map_err(anyhow::Error::from)?,
                id,
                title,
                status,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    citations.sort();
    citations.dedup();
    Ok(citations)
}

/// Captures that descend directly from `capture`.
pub async fn children(db: &SqlitePool, capture: &str) -> Result<Vec<String>, ApiError> {
    let rows = sqlx::query_scalar(
        "SELECT capture_id FROM capture_parents WHERE parent_id = ?1 ORDER BY capture_id",
    )
    .bind(capture)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Marks `capture` as removed with a tombstone inside `tx`.
pub async fn tombstone(
    tx: &mut SqliteConnection,
    capture: &str,
    actor: &str,
    reason: &str,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE captures SET deleted_at = ?2, deleted_by = ?3, delete_reason = ?4
         WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(capture)
    .bind(now)
    .bind(actor)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Moves open problems back to `needs-reprocess` when `capture` was their last live proof.
pub async fn reopen_uncited_problems(
    tx: &mut SqliteConnection,
    capture: &str,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE problems SET status = 'needs-reprocess', updated_at = ?2
         WHERE status IN ('open', 'in-progress')
           AND id IN (SELECT ps.problem_id FROM problem_selections ps
                      JOIN selection_items i ON i.selection_id = ps.selection_id
                      WHERE i.capture_id = ?1)
           AND NOT EXISTS (
             SELECT 1 FROM problem_selections ps
             JOIN selection_items i ON i.selection_id = ps.selection_id
             JOIN captures c ON c.id = i.capture_id
             WHERE ps.problem_id = problems.id AND i.mark = 'proof' AND c.deleted_at IS NULL)",
    )
    .bind(capture)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}
