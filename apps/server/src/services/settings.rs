//! Editing the upload limits: an administrator sets them for the server, and a project's owner
//! tightens them for the project. Setting nothing leaves the level above in charge.

use crate::{
    domain::{
        ApiError, Principal,
        limits::{key, positive, within_server},
        secret::now_ms,
    },
    repositories::{audit, settings},
    services::{access::require, limits},
    state::AppState,
};
use rupicola_api::{LimitSettings, LimitValue, Role};

/// Only an administrator changes the server; the name they act under.
fn administrator(principal: &Principal) -> Result<&str, ApiError> {
    let handle = principal.person()?;
    if principal.admin {
        Ok(handle)
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn server_view(state: &AppState) -> Result<LimitSettings, ApiError> {
    Ok(LimitSettings {
        set: limits::stored(state, None).await?.into_iter().collect(),
        effective: limits::server(state).await?,
    })
}

async fn project_view(state: &AppState, project: &str) -> Result<LimitSettings, ApiError> {
    Ok(LimitSettings {
        set: limits::stored(state, Some(project))
            .await?
            .into_iter()
            .collect(),
        effective: limits::effective(state, project).await?,
    })
}

/// The limits an administrator set for the server, and what they make.
pub async fn server(state: &AppState, principal: &Principal) -> Result<LimitSettings, ApiError> {
    administrator(principal)?;
    server_view(state).await
}

/// Sets a limit for the whole server. It may be higher or lower than the value the server
/// started with.
pub async fn set_server(
    state: &AppState,
    principal: &Principal,
    raw: &str,
    value: LimitValue,
) -> Result<LimitSettings, ApiError> {
    let actor = administrator(principal)?;
    let key = key(raw)?;
    let value = positive(value)?;
    let mut tx = state.db.begin().await?;
    settings::set_server(&mut *tx, key.as_str(), &value.to_string(), actor, now_ms()).await?;
    audit::record(
        &mut *tx,
        None,
        actor,
        &format!("Set the server's {key} to {value}"),
    )
    .await?;
    tx.commit().await?;
    server_view(state).await
}

/// The limit goes back to the value the server started with.
pub async fn reset_server(
    state: &AppState,
    principal: &Principal,
    raw: &str,
) -> Result<LimitSettings, ApiError> {
    let actor = administrator(principal)?;
    let key = key(raw)?;
    let mut tx = state.db.begin().await?;
    settings::reset_server(&mut *tx, key.as_str()).await?;
    audit::record(&mut *tx, None, actor, &format!("Reset the server's {key}")).await?;
    tx.commit().await?;
    server_view(state).await
}

/// The limits the owner set for the project, and what holds.
pub async fn project(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<LimitSettings, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    project_view(state, project).await
}

/// Tightens a limit for the project. A value above what the server allows is refused rather
/// than quietly ignored.
pub async fn set_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
    raw: &str,
    value: LimitValue,
) -> Result<LimitSettings, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.person()?;
    let key = key(raw)?;
    let value = positive(value)?;
    within_server(key, value, limits::server(state).await?.get(key))?;
    let mut tx = state.db.begin().await?;
    settings::set_project(
        &mut *tx,
        project,
        key.as_str(),
        &value.to_string(),
        actor,
        now_ms(),
    )
    .await?;
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Set {key} to {value}"),
    )
    .await?;
    tx.commit().await?;
    project_view(state, project).await
}

/// The project follows the server for this limit again.
pub async fn reset_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
    raw: &str,
) -> Result<LimitSettings, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.person()?;
    let key = key(raw)?;
    let mut tx = state.db.begin().await?;
    settings::reset_project(&mut *tx, project, key.as_str()).await?;
    audit::record(&mut *tx, Some(project), actor, &format!("Reset {key}")).await?;
    tx.commit().await?;
    project_view(state, project).await
}
