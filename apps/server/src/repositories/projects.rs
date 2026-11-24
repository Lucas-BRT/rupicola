//! The `projects` and `members` tables.

use crate::{domain::ApiError, repositories::corrupt};
use rupicola_api::{Member, Project, Role};
use sqlx::SqliteExecutor;

/// A `projects` row with the caller's role in it, if any.
#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    label: String,
    created_at: i64,
    role: Option<String>,
}

impl TryFrom<ProjectRow> for Project {
    type Error = ApiError;

    fn try_from(row: ProjectRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            label: row.label,
            created_at: row.created_at,
            role: row
                .role
                .as_deref()
                .map(str::parse)
                .transpose()
                .map_err(corrupt)?,
        })
    }
}

/// A `projects` row without a role.
#[derive(sqlx::FromRow)]
pub struct ProjectHead {
    pub label: String,
    pub created_at: i64,
}

/// A `members` row.
#[derive(sqlx::FromRow)]
struct MemberRow {
    handle: String,
    role: String,
}

impl MemberRow {
    fn into_member(self) -> Result<Member, ApiError> {
        Ok(Member {
            handle: self.handle,
            role: self.role.parse().map_err(corrupt)?,
        })
    }
}

pub async fn exists(db: impl SqliteExecutor<'_>, id: &str) -> sqlx::Result<bool> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?1")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(found.is_some())
}

pub async fn head(db: impl SqliteExecutor<'_>, id: &str) -> sqlx::Result<ProjectHead> {
    sqlx::query_as("SELECT label, created_at FROM projects WHERE id = ?1")
        .bind(id)
        .fetch_one(db)
        .await
}

/// The one project a device token belongs to.
pub async fn of_device(
    db: impl SqliteExecutor<'_>,
    project: Option<&str>,
) -> Result<Vec<Project>, ApiError> {
    let rows: Vec<ProjectRow> =
        sqlx::query_as("SELECT id, label, created_at, NULL AS role FROM projects WHERE id = ?1")
            .bind(project)
            .fetch_all(db)
            .await?;
    rows.into_iter().map(Project::try_from).collect()
}

/// Every project, which an administrator owns.
pub async fn all_owned(db: impl SqliteExecutor<'_>) -> Result<Vec<Project>, ApiError> {
    let rows: Vec<ProjectRow> = sqlx::query_as(
        "SELECT id, label, created_at, 'owner' AS role FROM projects ORDER BY label, id",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter().map(Project::try_from).collect()
}

/// The projects `handle` is a member of, with their role.
pub async fn of_member(
    db: impl SqliteExecutor<'_>,
    handle: Option<&str>,
) -> Result<Vec<Project>, ApiError> {
    let rows: Vec<ProjectRow> = sqlx::query_as(
        "SELECT p.id, p.label, p.created_at, m.role AS role FROM projects p
         JOIN members m ON m.project_id = p.id
         WHERE m.handle = ?1 ORDER BY p.label, p.id",
    )
    .bind(handle)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(Project::try_from).collect()
}

/// Inserts a project; `false` when the id is taken.
pub async fn insert(
    db: impl SqliteExecutor<'_>,
    id: &str,
    label: &str,
    created_at: i64,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query(
        "INSERT INTO projects (id, label, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(id)
    .bind(label)
    .bind(created_at)
    .execute(db)
    .await?
    .rows_affected();
    Ok(inserted > 0)
}

pub async fn rename(db: impl SqliteExecutor<'_>, id: &str, label: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE projects SET label = ?1 WHERE id = ?2")
        .bind(label)
        .bind(id)
        .execute(db)
        .await
        .map(drop)
}

pub async fn delete(db: impl SqliteExecutor<'_>, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM projects WHERE id = ?1")
        .bind(id)
        .execute(db)
        .await
        .map(drop)
}

/// The stored role of `handle` in `project`, as text.
pub async fn role(
    db: impl SqliteExecutor<'_>,
    project: &str,
    handle: Option<&str>,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT role FROM members WHERE project_id = ?1 AND handle = ?2")
        .bind(project)
        .bind(handle)
        .fetch_optional(db)
        .await
}

pub async fn members(db: impl SqliteExecutor<'_>, project: &str) -> Result<Vec<Member>, ApiError> {
    let rows: Vec<MemberRow> =
        sqlx::query_as("SELECT handle, role FROM members WHERE project_id = ?1 ORDER BY handle")
            .bind(project)
            .fetch_all(db)
            .await?;
    rows.into_iter().map(MemberRow::into_member).collect()
}

pub async fn put_member(
    db: impl SqliteExecutor<'_>,
    project: &str,
    handle: &str,
    role: Role,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO members (project_id, handle, role) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, handle) DO UPDATE SET role = excluded.role",
    )
    .bind(project)
    .bind(handle)
    .bind(role.as_str())
    .execute(db)
    .await
    .map(drop)
}

/// Removes a member; `false` when they were not one.
pub async fn delete_member(
    db: impl SqliteExecutor<'_>,
    project: &str,
    handle: &str,
) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM members WHERE project_id = ?1 AND handle = ?2")
        .bind(project)
        .bind(handle)
        .execute(db)
        .await?
        .rows_affected();
    Ok(removed > 0)
}

/// Owners of `project` other than `handle`.
pub async fn other_owners(
    db: impl SqliteExecutor<'_>,
    project: &str,
    handle: &str,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM members WHERE project_id = ?1 AND role = 'owner' AND handle <> ?2",
    )
    .bind(project)
    .bind(handle)
    .fetch_one(db)
    .await
}
