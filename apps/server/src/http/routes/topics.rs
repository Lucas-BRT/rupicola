//! `/topics/{id}`: one topic of a project's tree and the captures filed under it.

use super::super::handlers::topics;
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/{id}", routing::delete(topics::remove))
        .route("/{id}/name", routing::put(topics::rename))
        .route("/{id}/parent", routing::put(topics::move_to))
        .route("/{id}/captures", routing::get(topics::captures))
        .route(
            "/{id}/captures/{capture}",
            routing::put(topics::file).delete(topics::unfile),
        )
}
