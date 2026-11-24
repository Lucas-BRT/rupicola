//! The `topics` and `capture_topics` tables.

use crate::domain::topic::Change;
use rupicola_api::Topic;
use sqlx::SqliteExecutor;

/// What the database says when a name is already taken among siblings.
const NAME_TAKEN: &str = "UNIQUE constraint failed";
/// What the database says when a move would put a topic under itself.
const TOPIC_CYCLE: &str = "topic cycle";
const CHECK_FAILED: &str = "CHECK constraint failed";

/// A `topics` row with the number of captures filed directly under it.
#[derive(sqlx::FromRow)]
struct TopicRow {
    id: String,
    parent_id: Option<String>,
    name: String,
    position: i64,
    generation: i64,
    captures: i64,
}

impl TopicRow {
    fn into_topic(self, path: String) -> Topic {
        Topic {
            id: self.id,
            parent_id: self.parent_id,
            name: self.name,
            path,
            position: self.position,
            generation: self.generation,
            captures: u64::try_from(self.captures).unwrap_or_default(),
        }
    }
}

/// What a topic row selects, with its direct captures counted.
const COLUMNS: &str = "t.id, t.parent_id, t.name, t.position, t.generation,
   (SELECT count(*) FROM capture_topics c WHERE c.topic_id = t.id) AS captures";

/// An update read as the change it made.
fn change(result: sqlx::Result<sqlx::sqlite::SqliteQueryResult>) -> sqlx::Result<Change> {
    match result {
        Ok(done) if done.rows_affected() == 0 => Ok(Change::Stale),
        Ok(_) => Ok(Change::Done),
        Err(error) => {
            let text = error.to_string();
            if text.contains(NAME_TAKEN) {
                Ok(Change::NameTaken)
            } else if text.contains(TOPIC_CYCLE) || text.contains(CHECK_FAILED) {
                Ok(Change::Cycle)
            } else {
                Err(error)
            }
        }
    }
}

pub async fn project_of(db: impl SqliteExecutor<'_>, topic: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT project_id FROM topics WHERE id = ?1")
        .bind(topic)
        .fetch_optional(db)
        .await
}

/// A project's topics, with empty paths.
pub async fn of_project(db: impl SqliteExecutor<'_>, project: &str) -> sqlx::Result<Vec<Topic>> {
    let rows: Vec<TopicRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM topics t WHERE t.project_id = ?1"
    )))
    .bind(project)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.into_topic(String::new()))
        .collect())
}

/// One topic, with its path.
pub async fn one(db: &sqlx::SqlitePool, topic: &str) -> sqlx::Result<Option<Topic>> {
    let Some(row): Option<TopicRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM topics t WHERE t.id = ?1"
    )))
    .bind(topic)
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let names: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE up(id, parent_id, name, depth) AS (
           SELECT id, parent_id, name, 0 FROM topics WHERE id = ?1
           UNION ALL
           SELECT t.id, t.parent_id, t.name, up.depth + 1 FROM topics t JOIN up ON t.id = up.parent_id)
         SELECT name FROM up ORDER BY depth DESC",
    )
    .bind(topic)
    .fetch_all(db)
    .await?;
    Ok(Some(row.into_topic(crate::domain::topic::join(&names))))
}

/// The child of `parent` (or a root topic) called `name`, without regard to case.
pub async fn child(
    db: impl SqliteExecutor<'_>,
    project: &str,
    parent: Option<&str>,
    name: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT id FROM topics WHERE project_id = ?1 AND parent_id IS ?2
           AND lower(name) = lower(?3)",
    )
    .bind(project)
    .bind(parent)
    .bind(name)
    .fetch_optional(db)
    .await
}

/// Makes a topic; `false` when a sibling already has the name.
pub async fn insert(
    db: impl SqliteExecutor<'_>,
    id: &str,
    project: &str,
    parent: Option<&str>,
    name: &str,
    now: i64,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query(
        "INSERT INTO topics (id, project_id, parent_id, name, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(id)
    .bind(project)
    .bind(parent)
    .bind(name)
    .bind(now)
    .execute(db)
    .await;
    match inserted {
        Ok(_) => Ok(true),
        Err(error) if error.to_string().contains(NAME_TAKEN) => Ok(false),
        Err(error) => Err(error),
    }
}

pub async fn rename(
    db: impl SqliteExecutor<'_>,
    topic: &str,
    name: &str,
    generation: i64,
) -> sqlx::Result<Change> {
    change(
        sqlx::query(
            "UPDATE topics SET name = ?1, generation = generation + 1
             WHERE id = ?2 AND generation = ?3",
        )
        .bind(name)
        .bind(topic)
        .bind(generation)
        .execute(db)
        .await,
    )
}

pub async fn move_to(
    db: impl SqliteExecutor<'_>,
    topic: &str,
    parent: Option<&str>,
    generation: i64,
) -> sqlx::Result<Change> {
    change(
        sqlx::query(
            "UPDATE topics SET parent_id = ?1, generation = generation + 1
             WHERE id = ?2 AND generation = ?3",
        )
        .bind(parent)
        .bind(topic)
        .bind(generation)
        .execute(db)
        .await,
    )
}

pub async fn children(db: impl SqliteExecutor<'_>, topic: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM topics WHERE parent_id = ?1")
        .bind(topic)
        .fetch_one(db)
        .await
}

/// Removes a topic and every topic below it.
pub async fn delete_tree(db: impl SqliteExecutor<'_>, topic: &str) -> sqlx::Result<()> {
    sqlx::query(
        "WITH RECURSIVE tree(id) AS (
           SELECT ?1 UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
         DELETE FROM topics WHERE id IN (SELECT id FROM tree)",
    )
    .bind(topic)
    .execute(db)
    .await
    .map(drop)
}

/// The captures filed directly under a topic.
pub async fn direct_captures(
    db: impl SqliteExecutor<'_>,
    topic: &str,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT capture_id FROM capture_topics WHERE topic_id = ?1")
        .bind(topic)
        .fetch_all(db)
        .await
}

/// The captures filed under a topic or any topic below it.
pub async fn all_captures(db: impl SqliteExecutor<'_>, topic: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "WITH RECURSIVE tree(id) AS (
           SELECT ?1 UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
         SELECT DISTINCT c.capture_id FROM capture_topics c JOIN tree ON c.topic_id = tree.id",
    )
    .bind(topic)
    .fetch_all(db)
    .await
}

pub async fn file(
    db: impl SqliteExecutor<'_>,
    project: &str,
    topic: &str,
    capture: &str,
    by: Option<&str>,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO capture_topics (project_id, topic_id, capture_id, added_by, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(project)
    .bind(topic)
    .bind(capture)
    .bind(by)
    .bind(now)
    .execute(db)
    .await
    .map(drop)
}

pub async fn unfile(db: impl SqliteExecutor<'_>, topic: &str, capture: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM capture_topics WHERE topic_id = ?1 AND capture_id = ?2")
        .bind(topic)
        .bind(capture)
        .execute(db)
        .await
        .map(drop)
}
