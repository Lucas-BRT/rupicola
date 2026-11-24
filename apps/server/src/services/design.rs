//! Design records of a project.

use crate::{
    domain::{ApiError, Principal, design::validate, secret::now_ms},
    repositories::{audit, design},
    services::access::{require_publisher, require_reader},
    state::AppState,
};
use rupicola_api::{DesignEntry, NewDesignEntry, Role};

/// A project's records, newest first.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<DesignEntry>, ApiError> {
    require_reader(state, principal, project).await?;
    design::list(&state.db, project).await
}

/// Keeps a record; devices and reviewers may.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: NewDesignEntry,
) -> Result<DesignEntry, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    validate(&form)?;
    if let Some(artifact) = &form.evidence {
        crate::services::captures::require_project_artifact(state, project, artifact).await?;
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
