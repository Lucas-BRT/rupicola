//! `/jobs/{id}`: one capture job, and what a runner does with it.

use super::super::handlers::jobs;
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/{id}", routing::get(jobs::get))
        .route("/{id}/progress", routing::post(jobs::progress))
        .route("/{id}/complete", routing::post(jobs::complete))
        .route("/{id}/fail", routing::post(jobs::fail))
        .route("/{id}/cancel", routing::post(jobs::cancel))
}
