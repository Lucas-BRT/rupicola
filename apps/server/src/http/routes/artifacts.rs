//! `/artifacts`: one file of a capture.

use super::super::handlers::captures;
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new().route("/{id}", routing::get(captures::artifact))
}
