//! The `capture_parents` and `capture_parent_events` tables, and the graph they make.

use crate::domain::{ApiError, lineage::Contents};
use rupicola_api::{LineageEvent, LineageLink, LineageNode, ParentEdge, record::ParentRef};
use sqlx::{SqliteConnection, SqliteExecutor};

/// The error the database raises when an edge would close a loop.
const CYCLE_ERROR: &str = "capture lineage cycle";

/// A row of a lineage graph's nodes.
#[derive(sqlx::FromRow)]
struct NodeRow {
    id: String,
    scenario: String,
    series: Option<String>,
    created_at: i64,
    deleted: bool,
}

/// A `capture_parents` row seen from the child.
#[derive(sqlx::FromRow)]
struct LinkRow {
    capture_id: String,
    parent_id: String,
    kind: String,
    origin: String,
}

/// A `capture_parent_events` row.
#[derive(sqlx::FromRow)]
struct EventRow {
    generation: i64,
    actor: String,
    reason: String,
    before: String,
    after: String,
    created_at: i64,
}

/// A path and digest, or a facet key and value.
#[derive(sqlx::FromRow)]
struct Pair {
    key: String,
    value: String,
}

/// A capture and the project it belongs to.
#[derive(sqlx::FromRow)]
pub struct Owner {
    pub id: String,
    pub project_id: String,
}

/// The captures within `depth` steps of `capture`, up and down.
pub async fn reach(
    db: impl SqliteExecutor<'_>,
    capture: &str,
    depth: u32,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "WITH RECURSIVE
           up(id, depth) AS (
             SELECT ?1, 0
             UNION SELECT p.parent_id, up.depth + 1 FROM capture_parents p
               JOIN up ON p.capture_id = up.id WHERE up.depth < ?2),
           down(id, depth) AS (
             SELECT ?1, 0
             UNION SELECT p.capture_id, down.depth + 1 FROM capture_parents p
               JOIN down ON p.parent_id = down.id WHERE down.depth < ?2)
         SELECT id FROM up UNION SELECT id FROM down",
    )
    .bind(capture)
    .bind(depth)
    .fetch_all(db)
    .await
}

/// The captures named in `ids`, oldest first, removed ones marked.
pub async fn nodes(
    db: impl SqliteExecutor<'_>,
    ids: &[String],
) -> Result<Vec<LineageNode>, ApiError> {
    let rows: Vec<NodeRow> = sqlx::query_as(
        "SELECT id, scenario, series, created_at, deleted_at IS NOT NULL AS deleted FROM captures
         WHERE id IN (SELECT value FROM json_each(?1)) ORDER BY created_at, id",
    )
    .bind(serde_json::to_string(ids).map_err(anyhow::Error::from)?)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| LineageNode {
            id: row.id,
            scenario: row.scenario,
            series: row.series,
            created_at: row.created_at,
            deleted: row.deleted,
        })
        .collect())
}

/// The links between the captures named in `ids`.
pub async fn links(
    db: impl SqliteExecutor<'_>,
    ids: &[String],
) -> Result<Vec<LineageLink>, ApiError> {
    let rows: Vec<LinkRow> = sqlx::query_as(
        "SELECT capture_id, parent_id, kind, origin FROM capture_parents
         WHERE capture_id IN (SELECT value FROM json_each(?1))
           AND parent_id IN (SELECT value FROM json_each(?1))
         ORDER BY capture_id, position",
    )
    .bind(serde_json::to_string(ids).map_err(anyhow::Error::from)?)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LineageLink {
                child: row.capture_id,
                parent: row.parent_id,
                kind: row.kind.parse().map_err(anyhow::Error::from)?,
                origin: row.origin.parse().map_err(anyhow::Error::from)?,
            })
        })
        .collect()
}

/// The files of a capture by path, and its environment facets by key.
pub async fn contents(db: &sqlx::SqlitePool, capture: &str) -> sqlx::Result<Contents> {
    let files: Vec<Pair> =
        sqlx::query_as("SELECT path AS key, sha256 AS value FROM artifacts WHERE capture_id = ?1")
            .bind(capture)
            .fetch_all(db)
            .await?;
    let facets: Vec<Pair> =
        sqlx::query_as("SELECT key, value FROM capture_facets WHERE capture_id = ?1")
            .bind(capture)
            .fetch_all(db)
            .await?;
    let map = |pairs: Vec<Pair>| pairs.into_iter().map(|p| (p.key, p.value)).collect();
    Ok(Contents {
        files: map(files),
        facets: map(facets),
    })
}

/// The projects of two captures, removed or not.
pub async fn owners(db: impl SqliteExecutor<'_>, a: &str, b: &str) -> sqlx::Result<Vec<Owner>> {
    sqlx::query_as("SELECT id, project_id FROM captures WHERE id IN (?1, ?2)")
        .bind(a)
        .bind(b)
        .fetch_all(db)
        .await
}

/// The project of a live capture.
pub async fn live_project(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1 AND deleted_at IS NULL")
        .bind(capture)
        .fetch_optional(db)
        .await
}

pub async fn parents(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> Result<Vec<ParentEdge>, ApiError> {
    let rows: Vec<LinkRow> = sqlx::query_as(
        "SELECT capture_id, parent_id, kind, origin FROM capture_parents WHERE capture_id = ?1
         ORDER BY position, parent_id",
    )
    .bind(capture)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ParentEdge {
                id: row.parent_id,
                kind: row.kind.parse().map_err(anyhow::Error::from)?,
                origin: row.origin.parse().map_err(anyhow::Error::from)?,
            })
        })
        .collect()
}

pub async fn generation(db: impl SqliteExecutor<'_>, capture: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT parents_generation FROM captures WHERE id = ?1")
        .bind(capture)
        .fetch_one(db)
        .await
}

pub async fn history(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> Result<Vec<LineageEvent>, ApiError> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT generation, actor, reason, before, after, created_at
         FROM capture_parent_events WHERE capture_id = ?1 ORDER BY generation",
    )
    .bind(capture)
    .fetch_all(db)
    .await?;
    let corrupt = |e: serde_json::Error| ApiError::Internal(e.into());
    rows.into_iter()
        .map(|row| {
            Ok(LineageEvent {
                generation: row.generation,
                actor: row.actor,
                reason: row.reason,
                before: serde_json::from_str(&row.before).map_err(corrupt)?,
                after: serde_json::from_str(&row.after).map_err(corrupt)?,
                created_at: row.created_at,
            })
        })
        .collect()
}

/// Moves the lineage to the next generation if it is still `generation`; `false` otherwise.
pub async fn advance(
    db: impl SqliteExecutor<'_>,
    capture: &str,
    generation: i64,
) -> sqlx::Result<bool> {
    let moved = sqlx::query(
        "UPDATE captures SET parents_generation = parents_generation + 1
         WHERE id = ?1 AND parents_generation = ?2",
    )
    .bind(capture)
    .bind(generation)
    .execute(db)
    .await?
    .rows_affected();
    Ok(moved > 0)
}

/// Puts `parents` in place of the capture's edges. Returns the parent that would close a
/// loop, after which nothing more is written and the caller must roll back.
pub async fn replace(
    db: &mut SqliteConnection,
    project: &str,
    capture: &str,
    parents: &[ParentRef],
) -> Result<Option<String>, ApiError> {
    sqlx::query("DELETE FROM capture_parents WHERE capture_id = ?1")
        .bind(capture)
        .execute(&mut *db)
        .await?;
    for (position, parent) in parents.iter().enumerate() {
        let inserted = sqlx::query(
            "INSERT INTO capture_parents (project_id, capture_id, parent_id, kind, position, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, 'edit')",
        )
        .bind(project)
        .bind(capture)
        .bind(&parent.id)
        .bind(parent.kind.as_str())
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .execute(&mut *db)
        .await;
        match inserted {
            Ok(_) => {}
            Err(error) if error.to_string().contains(CYCLE_ERROR) => {
                return Ok(Some(parent.id.clone()));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

/// An edit of the parents, for the history.
pub struct Event<'a> {
    pub capture: &'a str,
    pub generation: i64,
    pub actor: &'a str,
    pub reason: &'a str,
    pub before: &'a [ParentRef],
    pub after: &'a [ParentRef],
    pub at: i64,
}

pub async fn insert_event(db: impl SqliteExecutor<'_>, event: &Event<'_>) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO capture_parent_events (id, capture_id, generation, actor, reason, before,
           after, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(event.capture)
    .bind(event.generation)
    .bind(event.actor)
    .bind(event.reason)
    .bind(serde_json::to_string(event.before).map_err(anyhow::Error::from)?)
    .bind(serde_json::to_string(event.after).map_err(anyhow::Error::from)?)
    .bind(event.at)
    .execute(db)
    .await?;
    Ok(())
}
