//! SQLite search query for captures.

use crate::domain::{ApiError, search::Search};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

/// The ids and instants of the captures matching `search`, one more than a page so the
/// caller knows whether another page follows.
pub async fn capture_ids(
    db: &SqlitePool,
    project: &str,
    search: &Search,
) -> Result<Vec<(String, i64)>, ApiError> {
    let mut sql = QueryBuilder::<Sqlite>::new(
        "SELECT c.id, c.created_at FROM captures c WHERE c.deleted_at IS NULL AND c.project_id = ",
    );
    sql.push_bind(project);
    if let Some((at, id)) = &search.cursor {
        sql.push(" AND (c.created_at, c.id) < (")
            .push_bind(*at)
            .push(", ")
            .push_bind(id.as_str())
            .push(")");
    }
    if let Some(topic) = &search.topic {
        if search.direct {
            sql.push(" AND c.id IN (SELECT capture_id FROM capture_topics WHERE topic_id = ")
                .push_bind(topic.as_str())
                .push(")");
        } else {
            sql.push(" AND c.id IN (WITH RECURSIVE tree(id) AS (SELECT ")
                .push_bind(topic.as_str())
                .push(
                    " UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
                     SELECT ct.capture_id FROM capture_topics ct JOIN tree ON ct.topic_id = tree.id)",
                );
        }
    }
    if let Some(series) = &search.series {
        sql.push(" AND c.series = ").push_bind(series.as_str());
    }
    if let Some(surface) = &search.surface {
        sql.push(
            " AND EXISTS (SELECT 1 FROM artifacts a JOIN surfaces s ON s.id = a.surface_id
               WHERE a.capture_id = c.id AND s.key = ",
        )
        .push_bind(surface.as_str())
        .push(")");
    }
    for (key, value) in &search.facets {
        sql.push(
            " AND EXISTS (SELECT 1 FROM capture_facets f WHERE f.capture_id = c.id AND f.key = ",
        )
        .push_bind(key.as_str())
        .push(" AND f.value = ")
        .push_bind(value.as_str())
        .push(")");
    }
    if let Some(status) = search.status {
        sql.push(" AND c.status = ").push_bind(status.as_str());
    }
    if let Some(inspection) = search.inspection {
        sql.push(" AND c.inspection = ")
            .push_bind(inspection.as_str());
    }
    if let Some(since) = search.since {
        sql.push(" AND c.created_at >= ").push_bind(since);
    }
    if search.unreferenced {
        sql.push(
            " AND NOT EXISTS (SELECT 1 FROM selection_items i WHERE i.capture_id = c.id)
              AND NOT EXISTS (SELECT 1 FROM artifacts a WHERE a.capture_id = c.id AND (
                EXISTS (SELECT 1 FROM fix_comparisons k WHERE a.id IN
                  (k.before_artifact_id, k.after_artifact_id, k.diff_artifact_id))
                OR EXISTS (SELECT 1 FROM fix_steps t WHERE t.evidence_artifact_id = a.id)
                OR EXISTS (SELECT 1 FROM design_entries d WHERE d.evidence = a.id)))",
        );
    }
    sql.push(" ORDER BY c.created_at DESC, c.id DESC LIMIT ")
        .push_bind(i64::from(search.limit) + 1);
    let rows = sql.build_query_as().fetch_all(db).await?;
    Ok(rows)
}
