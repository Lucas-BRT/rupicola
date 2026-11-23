//! `/captures/{id}`: one capture, its lineage, its citations and its findings.

use crate::{captures, lineage, removal, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/{id}", routing::get(captures::get).delete(removal::remove))
        .route("/{id}/findings", routing::post(captures::add_finding))
        .route("/{id}/citations", routing::get(removal::citations))
        .route(
            "/{id}/parents",
            routing::get(lineage::get).put(lineage::edit),
        )
        .route("/{id}/lineage", routing::get(lineage::graph))
        .route("/{a}/compare/{b}", routing::get(lineage::compare))
}
