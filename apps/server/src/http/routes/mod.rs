//! Every HTTP route, laid out like the URL tree: each file here owns one top-level
//! segment of the versioned API and nests what lives under it, so a path is found by
//! opening the file named after its first segment. Handlers stay in their feature
//! modules; these files only say which handler answers which method and path.

mod account;
mod artifacts;
mod blobs;
mod captures;
mod fixes;
mod jobs;
mod problems;
mod projects;
mod selections;
mod settings;
mod tokens;
mod topics;

#[cfg(test)]
pub use account::SIGN_IN_BURST;

use super::{mcp, security};
use crate::{domain::ApiError, state::Shared};
use axum::{Json, Router, routing};
use rupicola_api::{Health, HealthStatus};
use std::time::Duration;

/// Path of the unversioned health probe used by containers and load balancers.
pub const HEALTH_PATH: &str = "/healthz";
/// Path of the unversioned MCP endpoint, which MCP clients are configured with.
const MCP_PATH: &str = "/mcp";
/// The longest any upload may take, whatever a setting says: the handler enforces the
/// project's own limits, and this only stops a request that somehow outlives them.
const UPLOAD_BACKSTOP: Duration = Duration::from_hours(6);

/// Every route, wrapped in the request hardening from [`security`]. Versioned
/// routes are mounted once under [`rupicola_api::PREFIX`], so a future version is
/// a second `nest` rather than an edit of every path.
///
/// # Errors
///
/// Returns an error if the rate-limit configuration is invalid.
pub fn app(settings: &security::Security, state: Shared) -> anyhow::Result<Router> {
    let health_route = Router::new().route(HEALTH_PATH, routing::get(health));
    let routes = Router::new()
        .nest(rupicola_api::PREFIX, v1(settings)?)
        .merge(security::limit_body(
            Router::new().route(MCP_PATH, routing::post(mcp::serve)),
            settings.body_limit,
        ))
        .fallback(|| async { ApiError::NotFound })
        .with_state(state.clone());
    // Streamed uploads keep their own deadlines, from the project's limits, instead of the
    // request timeout every other route has.
    let uploads = Router::new()
        .nest(rupicola_api::PREFIX, blobs::uploads())
        .with_state(state);
    let timeout = Duration::from_secs(settings.request_timeout);
    Ok(
        security::harden_without_rate_limit(health_route, settings, timeout)
            .merge(security::harden(routes, settings, timeout)?)
            .merge(security::harden(uploads, settings, UPLOAD_BACKSTOP)?),
    )
}

/// Version 1 of the API, with paths relative to [`rupicola_api::PREFIX`].
fn v1(settings: &security::Security) -> anyhow::Result<Router<Shared>> {
    let api = Router::new()
        .route(
            "/capture-capabilities",
            routing::get(super::handlers::captures::capabilities),
        )
        .nest("/projects", projects::routes())
        .nest("/captures", captures::routes())
        .nest("/artifacts", artifacts::routes())
        .nest("/problems", problems::routes())
        .nest("/fixes", fixes::routes())
        .nest("/jobs", jobs::routes())
        .nest("/topics", topics::routes())
        .nest("/selections", selections::routes())
        .nest("/settings", settings::routes())
        .nest("/tokens", tokens::routes())
        .merge(account::routes())
        .merge(account::sign_in(settings)?);
    Ok(security::limit_body(api, settings.body_limit))
}

async fn health() -> Json<Health> {
    Json(Health {
        status: HealthStatus::Ok,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}
