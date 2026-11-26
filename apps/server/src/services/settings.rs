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
    DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE, DEFAULT_AUTO_RECAPTURE, DEFAULT_BASE_REF, LimitSettings,
    LimitValue, RecaptureSettings, RecaptureSettingsPatch, Role,
};

const KEY_AUTO_RECAPTURE: &str = "auto_recapture";
const KEY_BASE_REF: &str = "base_ref";
const KEY_ALLOW_AGENT_MISMATCH_OVERRIDE: &str = "allow_agent_mismatch_override";

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

fn decode_base_ref(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let decoded = serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_owned());
    let clean = decoded.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

fn encode_base_ref(base: &str) -> Result<String, ApiError> {
    crate::domain::fix::check_git_ref(base)?;
    serde_json::to_string(base.trim()).map_err(|_| ApiError::invalid("invalid base_ref"))
}

/// Base Git ref for `project` when composing comparison trees.
pub async fn project_base_ref(state: &AppState, project: &str) -> Result<String, ApiError> {
    let proj_rows = settings::project(&state.db, project).await?;
    if let Some(val) = find_setting(&proj_rows, KEY_BASE_REF).and_then(decode_base_ref) {
        return Ok(val);
    }
    Ok(DEFAULT_BASE_REF.to_owned())
}

/// Whether non-human tokens (`Machine` / `Device`) may pass `mismatch_override_reason` on `project`.
/// Project setting overrides the server setting; defaults to `false` (human sessions only).
pub async fn agent_mismatch_override_allowed(
    state: &AppState,
    project: &str,
) -> Result<bool, ApiError> {
    let proj_rows = settings::project(&state.db, project).await?;
    if let Some(val) = find_setting(&proj_rows, KEY_ALLOW_AGENT_MISMATCH_OVERRIDE)
        && let Ok(parsed) = val.parse::<bool>()
    {
        return Ok(parsed);
    }
    let srv_rows = settings::server(&state.db).await?;
    if let Some(val) = find_setting(&srv_rows, KEY_ALLOW_AGENT_MISMATCH_OVERRIDE)
        && let Ok(parsed) = val.parse::<bool>()
    {
        return Ok(parsed);
    }
    Ok(DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE)
}

/// Reads the effective capture settings (`auto_recapture`, `base_ref`, `allow_agent_mismatch_override`) for the server.
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
        .and_then(decode_base_ref)
        .unwrap_or_else(|| DEFAULT_BASE_REF.to_owned());
    let allow_agent_mismatch_override = find_setting(&srv_rows, KEY_ALLOW_AGENT_MISMATCH_OVERRIDE)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE);
    Ok(RecaptureSettings {
        auto_recapture,
        base_ref,
        allow_agent_mismatch_override,
    })
}

/// Updates server-wide capture settings (`auto_recapture`, `base_ref`, `allow_agent_mismatch_override`).
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
        let encoded = encode_base_ref(base)?;
        settings::set_server(&mut *tx, KEY_BASE_REF, &encoded, actor, now).await?;
    }
    if let Some(allow_override) = patch.allow_agent_mismatch_override {
        settings::set_server(
            &mut *tx,
            KEY_ALLOW_AGENT_MISMATCH_OVERRIDE,
            &allow_override.to_string(),
            actor,
            now,
        )
        .await?;
    }
    tx.commit().await?;
    capture_server(state, principal).await
}

/// Reads the effective capture settings (`auto_recapture`, `base_ref`, `allow_agent_mismatch_override`) for `project`.
pub async fn capture_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<RecaptureSettings, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    Ok(RecaptureSettings {
        auto_recapture: auto_recapture_enabled(state, project).await?,
        base_ref: project_base_ref(state, project).await?,
        allow_agent_mismatch_override: agent_mismatch_override_allowed(state, project).await?,
    })
}

/// Updates capture settings (`auto_recapture`, `base_ref`, `allow_agent_mismatch_override`) for `project`.
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
        let encoded = encode_base_ref(base)?;
        settings::set_project(&mut *tx, project, KEY_BASE_REF, &encoded, actor, now).await?;
    }
    if let Some(allow_override) = patch.allow_agent_mismatch_override {
        settings::set_project(
            &mut *tx,
            project,
            KEY_ALLOW_AGENT_MISMATCH_OVERRIDE,
            &allow_override.to_string(),
            actor,
            now,
        )
        .await?;
    }
    tx.commit().await?;
    capture_project(state, principal, project).await
}
