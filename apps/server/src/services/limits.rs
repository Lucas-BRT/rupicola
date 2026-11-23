//! The limits that hold for the server and for each project.

use crate::{
    domain::{
        ApiError, Principal,
        limits::{effective as tighten, setting},
    },
    repositories::settings::{self, SettingRow},
    services::access::require_publisher,
    state::AppState,
};
use rupicola_api::{LimitSetting, Limits, Role};

fn limits(rows: &[SettingRow]) -> Vec<(LimitSetting, u64)> {
    rows.iter()
        .filter_map(|row| setting(&row.key, &row.value))
        .collect()
}

/// The limits stored for the server, or for `project`.
pub async fn stored(
    state: &AppState,
    project: Option<&str>,
) -> Result<Vec<(LimitSetting, u64)>, ApiError> {
    let rows = match project {
        None => settings::server(&state.db).await?,
        Some(project) => settings::project(&state.db, project).await?,
    };
    Ok(limits(&rows))
}

/// The limits the server allows.
pub async fn server(state: &AppState) -> Result<Limits, ApiError> {
    Ok(state.limits.server(&stored(state, None).await?))
}

/// The limits that hold for `project`.
pub async fn effective(state: &AppState, project: &str) -> Result<Limits, ApiError> {
    Ok(tighten(
        server(state).await?,
        &stored(state, Some(project)).await?,
    ))
}

/// What the caller may upload to the project. A capture device uploads to its project,
/// so it reads the limits too.
pub async fn of_project(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Limits, ApiError> {
    require_publisher(state, principal, project, Role::Reader).await?;
    effective(state, project).await
}
