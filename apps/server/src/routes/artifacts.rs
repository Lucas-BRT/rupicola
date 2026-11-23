//! `/artifacts`: one file of a capture.

use crate::{captures, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new().route("/{id}", routing::get(captures::artifact))
}
