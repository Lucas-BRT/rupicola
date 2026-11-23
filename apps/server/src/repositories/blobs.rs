//! The `blobs` and `project_blobs` tables: which blobs exist and which projects know them.
//! A project only learns of a blob it uploaded or a capture of it cites, so asking what is
//! missing reveals nothing about other projects.

use crate::domain::{ApiError, secret::now_ms};

/// Records that `project` holds the blob `sha256` of `bytes` bytes.
///
/// # Errors
///
/// When the database refuses the rows.
pub async fn record(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    sha256: &str,
    bytes: u64,
) -> Result<(), ApiError> {
    let now = now_ms();
    sqlx::query("INSERT OR IGNORE INTO blobs (sha256, bytes, created_at) VALUES (?1, ?2, ?3)")
        .bind(sha256)
        .bind(i64::try_from(bytes).map_err(anyhow::Error::from)?)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO project_blobs (project_id, sha256, created_at)
         VALUES (?1, ?2, ?3)",
    )
    .bind(project)
    .bind(sha256)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Of `hashes`, those `project` does not hold, in the order given and each once.
///
/// # Errors
///
/// When the database cannot be read.
pub async fn missing(
    db: &sqlx::SqlitePool,
    project: &str,
    hashes: &[String],
) -> Result<Vec<String>, ApiError> {
    let asked = serde_json::to_string(hashes).map_err(anyhow::Error::from)?;
    let known: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT sha256 FROM project_blobs
         WHERE project_id = ?1 AND sha256 IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(asked)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();
    let mut seen = std::collections::HashSet::new();
    Ok(hashes
        .iter()
        .filter(|hash| !known.contains(*hash) && seen.insert(hash.as_str()))
        .cloned()
        .collect())
}
