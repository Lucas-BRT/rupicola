//! Credentials other than sessions in the `tokens` table, and the `invites` table.

use rupicola_api::{Credential, Invite, TokenKind};
use sqlx::SqliteExecutor;

/// A `tokens` row, as shown to its owner.
#[derive(sqlx::FromRow)]
struct CredentialRow {
    digest: String,
    kind: String,
    label: String,
    project_id: Option<String>,
    created_at: i64,
    expires_at: Option<i64>,
    last_used_at: Option<i64>,
}

impl CredentialRow {
    /// `None` when the stored kind is unknown.
    fn into_credential(self) -> Option<Credential> {
        Some(Credential {
            id: self.digest,
            kind: self.kind.parse().ok()?,
            label: self.label,
            project_id: self.project_id,
            created_at: self.created_at,
            expires_at: self.expires_at,
            last_used_at: self.last_used_at,
        })
    }
}

/// An `invites` row.
#[derive(sqlx::FromRow)]
struct InviteRow {
    digest: String,
    role: String,
    created_by: Option<String>,
    expires_at: i64,
}

impl InviteRow {
    /// `None` when the stored role is unknown.
    fn into_invite(self) -> Option<Invite> {
        Some(Invite {
            id: self.digest,
            role: self.role.parse().ok()?,
            created_by: self.created_by,
            expires_at: self.expires_at,
        })
    }
}

/// What a consumed invitation grants.
#[derive(sqlx::FromRow)]
pub struct InviteGrant {
    pub project_id: String,
    pub role: String,
}

pub async fn insert(
    db: impl SqliteExecutor<'_>,
    credential: &Credential,
    handle: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tokens (digest, kind, handle, project_id, label, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&credential.id)
    .bind(credential.kind.as_str())
    .bind(handle)
    .bind(&credential.project_id)
    .bind(&credential.label)
    .bind(credential.created_at)
    .execute(db)
    .await
    .map(drop)
}

/// A person's live sessions and machine tokens, newest first.
pub async fn of_person(
    db: impl SqliteExecutor<'_>,
    handle: &str,
    now: i64,
) -> sqlx::Result<Vec<Credential>> {
    let rows: Vec<CredentialRow> = sqlx::query_as(
        "SELECT digest, kind, label, project_id, created_at, expires_at, last_used_at
         FROM tokens WHERE handle = ?1 AND (expires_at IS NULL OR expires_at > ?2)
         ORDER BY created_at DESC",
    )
    .bind(handle)
    .bind(now)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(CredentialRow::into_credential)
        .collect())
}

/// A project's devices, newest first.
pub async fn devices(db: impl SqliteExecutor<'_>, project: &str) -> sqlx::Result<Vec<Credential>> {
    let rows: Vec<CredentialRow> = sqlx::query_as(
        "SELECT digest, kind, label, project_id, created_at, expires_at, last_used_at
         FROM tokens WHERE kind = ?1 AND project_id = ?2 ORDER BY created_at DESC",
    )
    .bind(TokenKind::Device.as_str())
    .bind(project)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(CredentialRow::into_credential)
        .collect())
}

/// Revokes one of a person's credentials, returning its label.
pub async fn delete_of_person(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    handle: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("DELETE FROM tokens WHERE digest = ?1 AND handle = ?2 RETURNING label")
        .bind(digest)
        .bind(handle)
        .fetch_optional(db)
        .await
}

/// Revokes one of a project's devices, returning its label.
pub async fn delete_device(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    project: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "DELETE FROM tokens WHERE digest = ?1 AND kind = ?2 AND project_id = ?3
         RETURNING label",
    )
    .bind(digest)
    .bind(TokenKind::Device.as_str())
    .bind(project)
    .fetch_optional(db)
    .await
}

pub async fn insert_invite(
    db: impl SqliteExecutor<'_>,
    invite: &Invite,
    project: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO invites (digest, project_id, role, created_by, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&invite.id)
    .bind(project)
    .bind(invite.role.as_str())
    .bind(&invite.created_by)
    .bind(invite.expires_at)
    .execute(db)
    .await
    .map(drop)
}

/// A project's pending invitations, the soonest to expire first.
pub async fn invites(
    db: impl SqliteExecutor<'_>,
    project: &str,
    now: i64,
) -> sqlx::Result<Vec<Invite>> {
    let rows: Vec<InviteRow> = sqlx::query_as(
        "SELECT digest, role, created_by, expires_at FROM invites
         WHERE project_id = ?1 AND expires_at > ?2 ORDER BY expires_at",
    )
    .bind(project)
    .bind(now)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(InviteRow::into_invite)
        .collect())
}

/// Withdraws an invitation; `false` when there was none.
pub async fn delete_invite(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    project: &str,
) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM invites WHERE digest = ?1 AND project_id = ?2")
        .bind(digest)
        .bind(project)
        .execute(db)
        .await?
        .rows_affected();
    Ok(removed > 0)
}

/// Consumes a live invitation, returning what it grants.
pub async fn take_invite(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    now: i64,
) -> sqlx::Result<Option<InviteGrant>> {
    sqlx::query_as(
        "DELETE FROM invites WHERE digest = ?1 AND expires_at > ?2
         RETURNING project_id, role",
    )
    .bind(digest)
    .bind(now)
    .fetch_optional(db)
    .await
}
