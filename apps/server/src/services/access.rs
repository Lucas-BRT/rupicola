//! The access rule every project-scoped use case starts with.

use crate::{
    domain::{Access, ApiError, Principal},
    repositories::projects,
    state::AppState,
};
use rupicola_api::{Role, TokenKind};

/// Resolves the caller's access to `project`.
///
/// # Errors
///
/// [`ApiError::NotFound`] when the project does not exist or the caller cannot see
/// it.
pub async fn access(
    state: &AppState,
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
    if !projects::exists(&state.db, project).await? {
        return Err(ApiError::NotFound);
    }
    if principal.admin {
        return Ok(Access::Person(Role::Owner));
    }
    projects::role(&state.db, project, principal.handle.as_deref())
        .await?
        .as_deref()
        .and_then(|text| text.parse().ok())
        .map(Access::Person)
        .ok_or(ApiError::NotFound)
}

/// Resolves the caller's access to `project` and requires at least `role`.
pub async fn require(
    state: &AppState,
    principal: &Principal,
    project: &str,
    role: Role,
) -> Result<(), ApiError> {
    access(state, principal, project).await?.require(role)
}

/// Requires a caller who may publish into `project`: its capture device, or a person
/// holding at least `role`.
pub async fn require_publisher(
    state: &AppState,
    principal: &Principal,
    project: &str,
    role: Role,
) -> Result<(), ApiError> {
    match access(state, principal, project).await? {
        Access::Device => Ok(()),
        person @ Access::Person(_) => person.require(role),
    }
}
