//! A project's scenarios: pushing a definition and reading it back, by name or by hash.

use crate::{
    domain::{ApiError, Principal, scenario::validate, secret::now_ms},
    repositories::scenarios,
    services::access::{require, require_publisher},
    state::AppState,
};
use rupicola_api::{Role, Scenario, ScenarioSummary, ScenarioVersion};

/// Keeps a definition. Returns the stored version and whether the hash was new.
pub async fn put(
    state: &AppState,
    principal: &Principal,
    project: &str,
    scenario: &Scenario,
) -> Result<(ScenarioVersion, bool), ApiError> {
    require(state, principal, project, Role::Reviewer).await?;
    validate(scenario)?;
    let hash = scenario.hash();
    let now = now_ms();
    let created = scenarios::put(
        &state.db,
        project,
        scenario,
        &hash,
        principal.handle.as_deref(),
        now,
    )
    .await?;
    let version = scenarios::version(&state.db, project, &scenario.name, &hash)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((version, created))
}

/// The latest version of each scenario. A capture device may read, so a runner can.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<ScenarioSummary>, ApiError> {
    require_publisher(state, principal, project, Role::Reader).await?;
    scenarios::list(&state.db, project).await
}

/// The latest version of `name`.
pub async fn latest(
    state: &AppState,
    principal: &Principal,
    project: &str,
    name: &str,
) -> Result<ScenarioVersion, ApiError> {
    require_publisher(state, principal, project, Role::Reader).await?;
    scenarios::latest(&state.db, project, name)
        .await?
        .ok_or(ApiError::NotFound)
}

/// One exact version of `name`.
pub async fn version(
    state: &AppState,
    principal: &Principal,
    project: &str,
    name: &str,
    hash: &str,
) -> Result<ScenarioVersion, ApiError> {
    require_publisher(state, principal, project, Role::Reader).await?;
    scenarios::version(&state.db, project, name, hash)
        .await?
        .ok_or(ApiError::NotFound)
}
