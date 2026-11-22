//! Projects, their members and their audit trail, plus the access rule every
//! project-scoped route uses.

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    AuditEvent, Member, MemberRole, NewProject, Project, ProjectUpdate, Role, TokenKind,
};
use sqlx::{Sqlite, SqliteExecutor, Transaction};

/// What the caller may do in a project. A person gets their role (an administrator
/// is an owner everywhere); a device only reaches its own project and may only
/// publish. Anything else is [`ApiError::NotFound`], so private project ids do not
/// leak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Person(Role),
    Device,
}

impl Access {
    /// Fails with [`ApiError::Forbidden`] unless a person holds at least `role`.
    pub fn require(self, role: Role) -> Result<(), ApiError> {
        match self {
            Self::Person(held) if held >= role => Ok(()),
            _ => Err(ApiError::Forbidden),
        }
    }
}

/// Resolves the caller's access to `project`.
///
/// # Errors
///
/// [`ApiError::NotFound`] when the project does not exist or the caller cannot see
/// it.
pub async fn access(
    state: &Shared,
    principal: &Principal,
    project: &str,
) -> Result<Access, ApiError> {
    if principal.kind == TokenKind::Device {
        return if principal.project.as_deref() == Some(project) {
            Ok(Access::Device)
        } else {
            Err(ApiError::NotFound)
        };
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?1")
        .bind(project)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }
    if principal.admin {
        return Ok(Access::Person(Role::Owner));
    }
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM members WHERE project_id = ?1 AND handle = ?2")
            .bind(project)
            .bind(&principal.handle)
            .fetch_optional(&state.db)
            .await?;
    role.as_deref()
        .and_then(parse_role)
        .map(Access::Person)
        .ok_or(ApiError::NotFound)
}

pub(crate) fn parse_role(text: &str) -> Option<Role> {
    match text {
        "owner" => Some(Role::Owner),
        "reviewer" => Some(Role::Reviewer),
        "reader" => Some(Role::Reader),
        _ => None,
    }
}

pub(crate) const fn role_text(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Reviewer => "reviewer",
        Role::Reader => "reader",
    }
}

/// Project ids are client-chosen slugs, so they double as safe path segments.
fn valid_id(id: &str) -> bool {
    (1..=100).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn clean_label(label: &str) -> Result<String, ApiError> {
    let label = label.trim();
    if (1..=100).contains(&label.chars().count()) {
        Ok(label.to_owned())
    } else {
        Err(ApiError::Invalid(
            "the project label must have 1–100 characters",
        ))
    }
}

/// The person behind a session or machine token; devices act for no one.
pub(crate) fn person(principal: &Principal) -> Result<&str, ApiError> {
    match (principal.kind, principal.handle.as_deref()) {
        (TokenKind::Session | TokenKind::Machine, Some(handle)) => Ok(handle),
        _ => Err(ApiError::Forbidden),
    }
}

/// Appends an audit event inside the caller's transaction.
pub async fn audit(
    db: impl SqliteExecutor<'_>,
    project: Option<&str>,
    actor: &str,
    event: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events (id, project_id, actor, event, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(project)
    .bind(actor)
    .bind(event)
    .bind(now_ms())
    .execute(db)
    .await?;
    Ok(())
}

/// `GET /v1/projects`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<Vec<Project>>, ApiError> {
    let rows: Vec<(String, String, i64, Option<String>)> = match principal.kind {
        TokenKind::Device => {
            sqlx::query_as("SELECT id, label, created_at, NULL FROM projects WHERE id = ?1")
                .bind(&principal.project)
                .fetch_all(&state.db)
                .await?
        }
        _ if principal.admin => {
            sqlx::query_as("SELECT id, label, created_at, 'owner' FROM projects ORDER BY label, id")
                .fetch_all(&state.db)
                .await?
        }
        _ => {
            sqlx::query_as(
                "SELECT p.id, p.label, p.created_at, m.role FROM projects p
                 JOIN members m ON m.project_id = p.id
                 WHERE m.handle = ?1 ORDER BY p.label, p.id",
            )
            .bind(&principal.handle)
            .fetch_all(&state.db)
            .await?
        }
    };
    Ok(Json(
        rows.into_iter()
            .map(|(id, label, created_at, role)| Project {
                id,
                label,
                created_at,
                role: role.as_deref().and_then(parse_role),
            })
            .collect(),
    ))
}

/// `POST /v1/projects`: the creator becomes its owner.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let handle = person(&principal)?;
    if !valid_id(&form.id) {
        return Err(ApiError::Invalid(
            "the project id must have 1–100 letters, digits, - or _",
        ));
    }
    let label = clean_label(&form.label)?;
    let created_at = now_ms();
    let mut tx = state.db.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO projects (id, label, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&form.id)
    .bind(&label)
    .bind(created_at)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Err(ApiError::Conflict("a project with this id already exists"));
    }
    sqlx::query("INSERT INTO members (project_id, handle, role) VALUES (?1, ?2, 'owner')")
        .bind(&form.id)
        .bind(handle)
        .execute(&mut *tx)
        .await?;
    audit(&mut *tx, Some(&form.id), handle, "Created the project").await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Project {
            id: form.id,
            label,
            created_at,
            role: Some(Role::Owner),
        }),
    ))
}

/// `GET /v1/projects/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    let access = access(&state, &principal, &id).await?;
    let (label, created_at): (String, i64) =
        sqlx::query_as("SELECT label, created_at FROM projects WHERE id = ?1")
            .bind(&id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(Project {
        id,
        label,
        created_at,
        role: match access {
            Access::Person(role) => Some(role),
            Access::Device => None,
        },
    }))
}

/// `PATCH /v1/projects/{id}`: owners rename the project.
pub async fn update(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<ProjectUpdate>,
) -> Result<Json<Project>, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?;
    let label = clean_label(&form.label)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE projects SET label = ?1 WHERE id = ?2")
        .bind(&label)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        Some(&id),
        actor,
        &format!("Renamed the project to {label}"),
    )
    .await?;
    tx.commit().await?;
    get(State(state), principal, Path(id)).await
}

/// `DELETE /v1/projects/{id}`: owners delete the project and everything in it.
pub async fn delete(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM projects WHERE id = ?1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit(&mut *tx, None, actor, &format!("Deleted project {id}")).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/projects/{id}/members`
pub async fn members(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Vec<Member>>, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Reader)?;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT handle, role FROM members WHERE project_id = ?1 ORDER BY handle")
            .bind(&id)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(handle, role)| {
                Some(Member {
                    handle,
                    role: parse_role(&role)?,
                })
            })
            .collect(),
    ))
}

/// `PUT /v1/projects/{id}/members/{handle}`: owners add a person or change their
/// role.
pub async fn put_member(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, handle)): Path<(String, String)>,
    Json(form): Json<MemberRole>,
) -> Result<Json<Member>, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?;
    let handle = handle.to_ascii_lowercase();
    let mut tx = state.db.begin().await?;
    let account: Option<i64> = sqlx::query_scalar("SELECT 1 FROM accounts WHERE handle = ?1")
        .bind(&handle)
        .fetch_optional(&mut *tx)
        .await?;
    if account.is_none() {
        return Err(ApiError::NotFound);
    }
    if form.role != Role::Owner {
        ensure_another_owner(&mut tx, &id, &handle).await?;
    }
    sqlx::query(
        "INSERT INTO members (project_id, handle, role) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, handle) DO UPDATE SET role = excluded.role",
    )
    .bind(&id)
    .bind(&handle)
    .bind(role_text(form.role))
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        Some(&id),
        actor,
        &format!("Set {handle} as {}", role_text(form.role)),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Member {
        handle,
        role: form.role,
    }))
}

/// `DELETE /v1/projects/{id}/members/{handle}`
pub async fn delete_member(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, handle)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Owner)?;
    let actor = person(&principal)?;
    let handle = handle.to_ascii_lowercase();
    let mut tx = state.db.begin().await?;
    ensure_another_owner(&mut tx, &id, &handle).await?;
    let removed = sqlx::query("DELETE FROM members WHERE project_id = ?1 AND handle = ?2")
        .bind(&id)
        .bind(&handle)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(ApiError::NotFound);
    }
    audit(&mut *tx, Some(&id), actor, &format!("Removed {handle}")).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Refuses a change that would leave the project without an owner.
async fn ensure_another_owner(
    tx: &mut Transaction<'_, Sqlite>,
    project: &str,
    leaving: &str,
) -> Result<(), ApiError> {
    let others: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM members WHERE project_id = ?1 AND role = 'owner' AND handle <> ?2",
    )
    .bind(project)
    .bind(leaving)
    .fetch_one(&mut **tx)
    .await?;
    if others == 0 {
        let is_owner: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM members WHERE project_id = ?1 AND handle = ?2 AND role = 'owner'",
        )
        .bind(project)
        .bind(leaving)
        .fetch_optional(&mut **tx)
        .await?;
        if is_owner.is_some() {
            return Err(ApiError::Conflict("a project must keep at least one owner"));
        }
    }
    Ok(())
}

/// `GET /v1/projects/{id}/audit`: the latest 200 events, newest first.
pub async fn audit_log(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    access(&state, &principal, &id)
        .await?
        .require(Role::Owner)?;
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, actor, event, occurred_at FROM audit_events
         WHERE project_id = ?1 ORDER BY occurred_at DESC, id DESC LIMIT 200",
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, actor, event, occurred_at)| AuditEvent {
                id,
                actor,
                event,
                occurred_at,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use rupicola_api::{AuditEvent, Member, Project, Role};

    /// Ana (no special rights) owns project `demo`; Bruno has an account only.
    async fn demo() -> (u16, String, String) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        seed_account(&state, "bruno", false).await;
        let ana = sign_in(port, "ana").await;
        let bruno = sign_in(port, "bruno").await;
        let created = call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":" Demo "}"#),
        )
        .await;
        assert_eq!(status(&created), 201, "{created}");
        (port, ana, bruno)
    }

    #[tokio::test]
    async fn the_creator_owns_the_project_and_outsiders_cannot_see_it() {
        let (port, ana, bruno) = demo().await;

        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&ana), None).await);
        assert_eq!(project.label, "Demo");
        assert_eq!(project.role, Some(Role::Owner));
        let mine: Vec<Project> = json(&call(port, "GET", "/v1/projects", Some(&ana), None).await);
        assert_eq!(mine.len(), 1);

        // To an outsider the project does not exist: 404, not 403.
        let hidden = call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await;
        assert_eq!(status(&hidden), 404, "{hidden}");
        let theirs: Vec<Project> =
            json(&call(port, "GET", "/v1/projects", Some(&bruno), None).await);
        assert_eq!(theirs, []);

        let duplicate = call(
            port,
            "POST",
            "/v1/projects",
            Some(&bruno),
            Some(r#"{"id":"demo","label":"Mine"}"#),
        )
        .await;
        assert_eq!(status(&duplicate), 409, "{duplicate}");
        let bad = call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"../x","label":"X"}"#),
        )
        .await;
        assert_eq!(status(&bad), 422, "{bad}");
    }

    #[tokio::test]
    async fn roles_limit_what_members_may_change() {
        let (port, ana, bruno) = demo().await;
        let added = call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&added), 200, "{added}");

        let seen: Project = json(&call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await);
        assert_eq!(seen.role, Some(Role::Reader));
        let rename = call(
            port,
            "PATCH",
            "/v1/projects/demo",
            Some(&bruno),
            Some(r#"{"label":"Taken"}"#),
        )
        .await;
        assert_eq!(status(&rename), 403, "{rename}");
        let promote = call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&bruno),
            Some(r#"{"role":"owner"}"#),
        )
        .await;
        assert_eq!(status(&promote), 403, "{promote}");

        let members: Vec<Member> =
            json(&call(port, "GET", "/v1/projects/demo/members", Some(&bruno), None).await);
        assert_eq!(
            members,
            [
                Member {
                    handle: "ana".into(),
                    role: Role::Owner
                },
                Member {
                    handle: "bruno".into(),
                    role: Role::Reader
                },
            ]
        );
        let ghost = call(
            port,
            "PUT",
            "/v1/projects/demo/members/nobody",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&ghost), 404, "{ghost}");
    }

    #[tokio::test]
    async fn a_project_always_keeps_an_owner() {
        let (port, ana, _) = demo().await;
        let demote = call(
            port,
            "PUT",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&demote), 409, "{demote}");
        let leave = call(
            port,
            "DELETE",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&leave), 409, "{leave}");

        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"owner"}"#),
        )
        .await;
        let leave = call(
            port,
            "DELETE",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&leave), 204, "{leave}");
    }

    #[tokio::test]
    async fn owners_rename_and_delete_with_an_audit_trail() {
        let (port, ana, _) = demo().await;
        let renamed: Project = json(
            &call(
                port,
                "PATCH",
                "/v1/projects/demo",
                Some(&ana),
                Some(r#"{"label":"Renamed"}"#),
            )
            .await,
        );
        assert_eq!(renamed.label, "Renamed");

        let events: Vec<AuditEvent> =
            json(&call(port, "GET", "/v1/projects/demo/audit", Some(&ana), None).await);
        let described: Vec<_> = events.iter().map(|event| event.event.as_str()).collect();
        assert_eq!(
            described,
            ["Renamed the project to Renamed", "Created the project"]
        );
        assert!(events.iter().all(|event| event.actor == "ana"));

        let deleted = call(port, "DELETE", "/v1/projects/demo", Some(&ana), None).await;
        assert_eq!(status(&deleted), 204, "{deleted}");
        let gone = call(port, "GET", "/v1/projects/demo", Some(&ana), None).await;
        assert_eq!(status(&gone), 404, "{gone}");
    }

    #[tokio::test]
    async fn an_administrator_sees_every_project_as_owner() {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        seed_account(&state, "root", true).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        let root = sign_in(port, "root").await;

        let all: Vec<Project> = json(&call(port, "GET", "/v1/projects", Some(&root), None).await);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role, Some(Role::Owner));
    }
}
