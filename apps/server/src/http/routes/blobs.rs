//! Blob uploads, mounted apart from the other routes because their body limits and
//! deadlines differ.

use super::super::handlers::captures;
use crate::state::Shared;
use axum::{Router, routing};

/// Streamed uploads into a project, under the project's own limits and deadlines.
pub fn uploads() -> Router<Shared> {
    Router::new().route(
        "/projects/{id}/blobs/{sha256}",
        routing::put(captures::upload),
    )
}
