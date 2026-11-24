//! `scenario_versions`: capture definitions kept by the hash of the parsed definition.

use crate::domain::ApiError;
use rupicola_api::{Scenario, ScenarioSummary, ScenarioVersion};
use sqlx::SqlitePool;

/// A `scenario_versions` row.
#[derive(sqlx::FromRow)]
struct VersionRow {
    hash: String,
    definition: String,
    author: Option<String>,
    created_at: i64,
}

impl TryFrom<VersionRow> for ScenarioVersion {
    type Error = ApiError;

    fn try_from(row: VersionRow) -> Result<Self, ApiError> {
        Ok(Self {
            hash: row.hash,
            scenario: serde_json::from_str(&row.definition).map_err(anyhow::Error::from)?,
            author: row.author,
            created_at: row.created_at,
        })
    }
}

/// Stores a version, or makes the existing one the latest again. Returns whether the hash
/// was new to the project's scenario.
pub async fn put(
    db: &SqlitePool,
    project: &str,
    scenario: &Scenario,
    hash: &str,
    author: Option<&str>,
    now: i64,
) -> Result<bool, ApiError> {
    let definition = serde_json::to_string(scenario).map_err(anyhow::Error::from)?;
    let created = sqlx::query(
        "INSERT INTO scenario_versions (project_id, name, hash, definition, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (project_id, name, hash) DO NOTHING",
    )
    .bind(project)
    .bind(&scenario.name)
    .bind(hash)
    .bind(definition)
    .bind(author)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        > 0;
    if !created {
        sqlx::query(
            "UPDATE scenario_versions SET created_at = ?4
             WHERE project_id = ?1 AND name = ?2 AND hash = ?3",
        )
        .bind(project)
        .bind(&scenario.name)
        .bind(hash)
        .bind(now)
        .execute(db)
        .await?;
    }
    Ok(created)
}

/// The most recently pushed version of `name`.
pub async fn latest(
    db: &SqlitePool,
    project: &str,
    name: &str,
) -> Result<Option<ScenarioVersion>, ApiError> {
    let row: Option<VersionRow> = sqlx::query_as(
        "SELECT hash, definition, author, created_at FROM scenario_versions
         WHERE project_id = ?1 AND name = ?2 ORDER BY created_at DESC, hash LIMIT 1",
    )
    .bind(project)
    .bind(name)
    .fetch_optional(db)
    .await?;
    row.map(ScenarioVersion::try_from).transpose()
}

/// One exact version of `name`.
pub async fn version(
    db: &SqlitePool,
    project: &str,
    name: &str,
    hash: &str,
) -> Result<Option<ScenarioVersion>, ApiError> {
    let row: Option<VersionRow> = sqlx::query_as(
        "SELECT hash, definition, author, created_at FROM scenario_versions
         WHERE project_id = ?1 AND name = ?2 AND hash = ?3",
    )
    .bind(project)
    .bind(name)
    .bind(hash)
    .fetch_optional(db)
    .await?;
    row.map(ScenarioVersion::try_from).transpose()
}

/// The latest version of each of the project's scenarios, by name.
pub async fn list(db: &SqlitePool, project: &str) -> Result<Vec<ScenarioSummary>, ApiError> {
    let rows: Vec<VersionRow> = sqlx::query_as(
        "SELECT hash, definition, author, created_at FROM scenario_versions v
         WHERE project_id = ?1
           AND created_at = (SELECT MAX(created_at) FROM scenario_versions
                             WHERE project_id = v.project_id AND name = v.name)
         ORDER BY name, hash LIMIT ?2",
    )
    .bind(project)
    .bind(super::MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    let mut summaries: Vec<ScenarioSummary> = Vec::with_capacity(rows.len());
    for row in rows {
        let version = ScenarioVersion::try_from(row)?;
        // Two versions pushed in the same millisecond: the list keeps the first by hash.
        if summaries
            .last()
            .is_some_and(|s| s.name == version.scenario.name)
        {
            continue;
        }
        summaries.push(ScenarioSummary {
            name: version.scenario.name.clone(),
            hash: version.hash,
            url: version.scenario.url.clone(),
            cells: version.scenario.matrix.cells(),
            created_at: version.created_at,
        });
    }
    Ok(summaries)
}
