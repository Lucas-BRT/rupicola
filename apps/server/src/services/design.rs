//! Design records of a project.

use crate::{
    domain::{Access, ApiError, Principal, design::validate, secret::now_ms},
    repositories::{audit, design},
    services::access::{access, require},
    state::AppState,
};
use rupicola_api::{DesignEntry, NewDesignEntry, Role};

/// A project's records, newest first.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<DesignEntry>, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    design::list(&state.db, project).await
}

/// Keeps a record; devices and reviewers may.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: NewDesignEntry,
) -> Result<DesignEntry, ApiError> {
    match access(state, principal, project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    validate(&form)?;
    if let Some(artifact) = &form.evidence {
        crate::captures::require_project_artifact(state, project, artifact).await?;
    }
    if let Some(older) = &form.supersedes
        && !design::belongs(&state.db, older, project).await?
    {
        return Err(ApiError::invalid(
            "a record can only replace one of the same project",
        ));
    }
    let id = uuid::Uuid::now_v7().to_string();
    let mut tx = state.db.begin().await?;
    design::insert(
        &mut *tx,
        &id,
        project,
        &form,
        principal.handle.as_deref(),
        now_ms(),
    )
    .await?;
    audit::record(
        &mut *tx,
        Some(project),
        principal.actor(),
        &format!("Recorded design {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    design::get(&state.db, &id).await
}
