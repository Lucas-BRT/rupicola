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
use rupicola_api::{
    DEFAULT_AUTO_RECAPTURE, DEFAULT_BASE_REF, LimitSettings, LimitValue, RecaptureSettings,
    RecaptureSettingsPatch, Role,
};

const KEY_AUTO_RECAPTURE: &str = "auto_recapture";
const KEY_BASE_REF: &str = "base_ref";

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
    let stored = limits::stored(state, None).await?;
    let effective = state.limits.server(&stored);
    Ok(LimitSettings {
        set: stored.into_iter().collect(),
        effective,
    })
}

async fn project_view(state: &AppState, project: &str) -> Result<LimitSettings, ApiError> {
    let stored = limits::stored(state, Some(project)).await?;
    let server = limits::server(state).await?;
    let effective = crate::domain::limits::effective(server, &stored);
    Ok(LimitSettings {
        set: stored.into_iter().collect(),
        effective,
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

fn find_setting<'a>(rows: &'a [settings::SettingRow], key: &str) -> Option<&'a str> {
    rows.iter()
        .find(|row| row.key == key)
        .map(|row| row.value.as_str())
}

/// Whether automatic recapture on fix approval is enabled for `project`.
pub async fn auto_recapture_enabled(state: &AppState, project: &str) -> Result<bool, ApiError> {
    let proj_rows = settings::project(&state.db, project).await?;
    if let Some(val) = find_setting(&proj_rows, KEY_AUTO_RECAPTURE)
        && let Ok(parsed) = val.parse::<bool>()
    {
        return Ok(parsed);
    }
    let srv_rows = settings::server(&state.db).await?;
    if let Some(val) = find_setting(&srv_rows, KEY_AUTO_RECAPTURE)
        && let Ok(parsed) = val.parse::<bool>()
    {
        return Ok(parsed);
    }
    Ok(DEFAULT_AUTO_RECAPTURE)
}

/// Base Git ref for `project` when composing comparison trees.
pub async fn project_base_ref(state: &AppState, project: &str) -> Result<String, ApiError> {
    let proj_rows = settings::project(&state.db, project).await?;
    if let Some(val) = find_setting(&proj_rows, KEY_BASE_REF)
        && !val.trim().is_empty()
    {
        return Ok(val.trim().to_owned());
    }
    Ok(DEFAULT_BASE_REF.to_owned())
}

/// Reads the effective capture settings (`auto_recapture`, `base_ref`) for the server.
pub async fn capture_server(
    state: &AppState,
    principal: &Principal,
) -> Result<RecaptureSettings, ApiError> {
    administrator(principal)?;
    let srv_rows = settings::server(&state.db).await?;
    let auto_recapture = find_setting(&srv_rows, KEY_AUTO_RECAPTURE)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(DEFAULT_AUTO_RECAPTURE);
    let base_ref = find_setting(&srv_rows, KEY_BASE_REF)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(DEFAULT_BASE_REF)
        .to_owned();
    Ok(RecaptureSettings {
        auto_recapture,
        base_ref,
    })
}

/// Updates server-wide capture settings (`auto_recapture`, `base_ref`).
pub async fn update_capture_server(
    state: &AppState,
    principal: &Principal,
    patch: &RecaptureSettingsPatch,
) -> Result<RecaptureSettings, ApiError> {
    let actor = administrator(principal)?;
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    if let Some(enabled) = patch.auto_recapture {
        settings::set_server(
            &mut *tx,
            KEY_AUTO_RECAPTURE,
            &enabled.to_string(),
            actor,
            now,
        )
        .await?;
    }
    if let Some(base) = &patch.base_ref {
        crate::domain::fix::check_git_ref(base)?;
        settings::set_server(&mut *tx, KEY_BASE_REF, base.trim(), actor, now).await?;
    }
    tx.commit().await?;
    capture_server(state, principal).await
}

/// Reads the effective capture settings (`auto_recapture`, `base_ref`) for `project`.
pub async fn capture_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<RecaptureSettings, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    Ok(RecaptureSettings {
        auto_recapture: auto_recapture_enabled(state, project).await?,
        base_ref: project_base_ref(state, project).await?,
    })
}

/// Updates capture settings (`auto_recapture`, `base_ref`) for `project`.
pub async fn update_capture_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
    patch: &RecaptureSettingsPatch,
) -> Result<RecaptureSettings, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.person()?;
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    if let Some(enabled) = patch.auto_recapture {
        settings::set_project(
            &mut *tx,
            project,
            KEY_AUTO_RECAPTURE,
            &enabled.to_string(),
            actor,
            now,
        )
        .await?;
    }
    if let Some(base) = &patch.base_ref {
        crate::domain::fix::check_git_ref(base)?;
        settings::set_project(&mut *tx, project, KEY_BASE_REF, base.trim(), actor, now).await?;
    }
    tx.commit().await?;
    capture_project(state, principal, project).await
}
