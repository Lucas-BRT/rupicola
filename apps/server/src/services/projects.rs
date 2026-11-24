//! Projects, their members and their audit trail.

use crate::{
    domain::{
        ApiError, Principal,
        account::normal_handle,
        project::{check_id, clean_label},
        secret::now_ms,
    },
    repositories::{accounts, audit, projects},
    services::access::{access, require},
    state::AppState,
};
use rupicola_api::{AuditEvent, Member, NewProject, Project, Role, TokenKind};
use sqlx::SqliteConnection;

/// Audit events returned by one request.
const AUDIT_PAGE: i64 = 200;

/// The projects the caller can see.
pub async fn list(state: &AppState, principal: &Principal) -> Result<Vec<Project>, ApiError> {
    Ok(match principal.kind {
        TokenKind::Device => projects::of_device(&state.db, principal.project.as_deref()).await?,
        _ if principal.admin => projects::all_owned(&state.db).await?,
        _ => projects::of_member(&state.db, principal.handle.as_deref()).await?,
    })
}

/// Creates a project; the creator becomes its owner.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    form: NewProject,
) -> Result<Project, ApiError> {
    let handle = principal.person()?;
    check_id(&form.id)?;
    let label = clean_label(&form.label)?;
    let created_at = now_ms();
    let mut tx = state.db.begin().await?;
    if !projects::insert(&mut *tx, &form.id, &label, created_at).await? {
        return Err(ApiError::Conflict("a project with this id already exists"));
    }
    projects::put_member(&mut *tx, &form.id, handle, Role::Owner).await?;
    audit::record(&mut *tx, Some(&form.id), handle, "Created the project").await?;
    tx.commit().await?;
    Ok(Project {
        id: form.id,
        label,
        created_at,
        role: Some(Role::Owner),
    })
}

pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Project, ApiError> {
    let access = access(state, principal, id).await?;
    let head = projects::head(&state.db, id).await?;
    Ok(Project {
        id: id.to_owned(),
        label: head.label,
        created_at: head.created_at,
        role: access.role(),
    })
}

/// Owners rename the project.
pub async fn rename(
    state: &AppState,
    principal: &Principal,
    id: &str,
    label: &str,
) -> Result<Project, ApiError> {
    require(state, principal, id, Role::Owner).await?;
    let actor = principal.person()?;
    let label = clean_label(label)?;
    let mut tx = state.db.begin().await?;
    projects::rename(&mut *tx, id, &label).await?;
    audit::record(
        &mut *tx,
        Some(id),
        actor,
        &format!("Renamed the project to {label}"),
    )
    .await?;
    tx.commit().await?;
    get(state, principal, id).await
}

/// Owners delete the project and everything in it.
pub async fn delete(state: &AppState, principal: &Principal, id: &str) -> Result<(), ApiError> {
    require(state, principal, id, Role::Owner).await?;
    let actor = principal.person()?;
    let mut tx = state.db.begin().await?;
    projects::delete(&mut *tx, id).await?;
    audit::record(&mut *tx, None, actor, &format!("Deleted project {id}")).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn members(
    state: &AppState,
    principal: &Principal,
    id: &str,
) -> Result<Vec<Member>, ApiError> {
    require(state, principal, id, Role::Reader).await?;
    projects::members(&state.db, id).await
}

/// Owners add a person or change their role.
pub async fn put_member(
    state: &AppState,
    principal: &Principal,
    id: &str,
    handle: &str,
    role: Role,
) -> Result<Member, ApiError> {
    require(state, principal, id, Role::Owner).await?;
    let actor = principal.person()?;
    let handle = normal_handle(handle);
    let mut tx = state.db.begin().await?;
    if !accounts::exists(&mut *tx, &handle).await? {
        return Err(ApiError::NotFound);
    }
    if role != Role::Owner {
        ensure_another_owner(&mut tx, id, &handle).await?;
    }
    projects::put_member(&mut *tx, id, &handle, role).await?;
    audit::record(
        &mut *tx,
        Some(id),
        actor,
        &format!("Set {handle} as {}", role.as_str()),
    )
    .await?;
    tx.commit().await?;
    Ok(Member { handle, role })
}

pub async fn delete_member(
    state: &AppState,
    principal: &Principal,
    id: &str,
    handle: &str,
) -> Result<(), ApiError> {
    require(state, principal, id, Role::Owner).await?;
    let actor = principal.person()?;
    let handle = normal_handle(handle);
    let mut tx = state.db.begin().await?;
    ensure_another_owner(&mut tx, id, &handle).await?;
    if !projects::delete_member(&mut *tx, id, &handle).await? {
        return Err(ApiError::NotFound);
    }
    audit::record(&mut *tx, Some(id), actor, &format!("Removed {handle}")).await?;
    tx.commit().await?;
    Ok(())
}

/// Refuses a change that would leave the project without an owner.
async fn ensure_another_owner(
    db: &mut SqliteConnection,
    project: &str,
    leaving: &str,
) -> Result<(), ApiError> {
    if projects::other_owners(&mut *db, project, leaving).await? == 0
        && projects::role(&mut *db, project, Some(leaving))
            .await?
            .as_deref()
            == Some(Role::Owner.as_str())
    {
        return Err(ApiError::Conflict("a project must keep at least one owner"));
    }
    Ok(())
}

/// The latest [`AUDIT_PAGE`] events, newest first.
pub async fn audit_log(
    state: &AppState,
    principal: &Principal,
    id: &str,
) -> Result<Vec<AuditEvent>, ApiError> {
    require(state, principal, id, Role::Owner).await?;
    Ok(audit::latest(&state.db, id, AUDIT_PAGE).await?)
}
