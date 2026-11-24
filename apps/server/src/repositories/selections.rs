//! SQLite queries for `selections` and `selection_items`.

use crate::domain::ApiError;
use rupicola_api::{Fragment, Selection, SelectionItem};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

/// A `selections` row.
#[derive(sqlx::FromRow)]
struct HeadRow {
    id: String,
    label: String,
    author: Option<String>,
    created_at: i64,
}

/// A `selection_items` row.
#[derive(sqlx::FromRow)]
struct ItemRow {
    selection_id: String,
    capture_id: String,
    artifact_id: Option<String>,
    mark: String,
    fragment: Option<String>,
}

/// Inserts a selection header row inside `tx`.
pub async fn insert(
    tx: &mut SqliteConnection,
    id: &str,
    project: &str,
    label: &str,
    author: Option<&str>,
    created_at: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO selections (id, project_id, label, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(id)
    .bind(project)
    .bind(label)
    .bind(author)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The artifact an anchor names, by id or by path, after checking it belongs to the anchor's
/// capture and the capture to the project; `None` for a whole capture.
pub async fn resolve_item(
    tx: &mut SqliteConnection,
    project: &str,
    item: &SelectionItem,
) -> Result<Option<String>, ApiError> {
    let capture: Option<String> = sqlx::query_scalar(
        "SELECT id FROM captures WHERE id = ?1 AND project_id = ?2 AND deleted_at IS NULL",
    )
    .bind(&item.capture)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    if capture.is_none() {
        return Err(ApiError::invalid(format!(
            "{} is not a capture of this project",
            item.capture
        )));
    }
    let found: Option<String> = match (&item.artifact, &item.path) {
        (None, None) => return Ok(None),
        (Some(artifact), _) => {
            sqlx::query_scalar("SELECT id FROM artifacts WHERE id = ?1 AND capture_id = ?2")
                .bind(artifact)
                .bind(&item.capture)
                .fetch_optional(&mut *tx)
                .await?
        }
        (None, Some(path)) => {
            sqlx::query_scalar("SELECT id FROM artifacts WHERE path = ?1 AND capture_id = ?2")
                .bind(path)
                .bind(&item.capture)
                .fetch_optional(&mut *tx)
                .await?
        }
    };
    found.map(Some).ok_or_else(|| {
        ApiError::invalid(format!(
            "capture {} has no file {}",
            item.capture,
            item.artifact
                .as_deref()
                .or(item.path.as_deref())
                .unwrap_or_default()
        ))
    })
}

/// Anchor item row to insert under a selection.
pub struct NewSelectionItem<'a> {
    pub project: &'a str,
    pub selection_id: &'a str,
    pub position: i64,
    pub capture_id: &'a str,
    pub artifact_id: Option<&'a str>,
    pub mark: &'a str,
    pub fragment: Option<&'a str>,
}

/// Inserts one anchor item for a selection inside `tx`.
pub async fn insert_item(
    tx: &mut SqliteConnection,
    item: NewSelectionItem<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO selection_items (project_id, selection_id, position, capture_id,
           artifact_id, mark, fragment)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(item.project)
    .bind(item.selection_id)
    .bind(item.position)
    .bind(item.capture_id)
    .bind(item.artifact_id)
    .bind(item.mark)
    .bind(item.fragment)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The project that owns `id`, if the selection exists.
pub async fn project_of(db: &SqlitePool, id: &str) -> Result<Option<String>, ApiError> {
    let project = sqlx::query_scalar("SELECT project_id FROM selections WHERE id = ?1")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(project)
}

/// The selections named in `ids`, in that order.
///
/// # Errors
///
/// When the database cannot be read or holds a malformed fragment.
pub async fn load(db: &SqlitePool, ids: &[String]) -> Result<Vec<Selection>, ApiError> {
    let wanted = serde_json::to_string(ids).map_err(anyhow::Error::from)?;
    let heads: Vec<HeadRow> = sqlx::query_as(
        "SELECT id, label, author, created_at FROM selections
         WHERE id IN (SELECT value FROM json_each(?1))",
    )
    .bind(&wanted)
    .fetch_all(db)
    .await?;
    let rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT selection_id, capture_id, artifact_id, mark, fragment FROM selection_items
         WHERE selection_id IN (SELECT value FROM json_each(?1)) ORDER BY selection_id, position",
    )
    .bind(&wanted)
    .fetch_all(db)
    .await?;
    let mut items: HashMap<String, Vec<SelectionItem>> = HashMap::new();
    for row in rows {
        let fragment: Option<Fragment> = row
            .fragment
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(anyhow::Error::from)?;
        items
            .entry(row.selection_id)
            .or_default()
            .push(SelectionItem {
                capture: row.capture_id,
                artifact: row.artifact_id,
                path: None,
                mark: row.mark.parse().map_err(anyhow::Error::from)?,
                fragment,
            });
    }
    let mut by_id: HashMap<String, Selection> = heads
        .into_iter()
        .map(|head| {
            let anchors = items.remove(&head.id).unwrap_or_default();
            (
                head.id.clone(),
                Selection {
                    id: head.id,
                    label: head.label,
                    author: head.author,
                    created_at: head.created_at,
                    items: anchors,
                },
            )
        })
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}
