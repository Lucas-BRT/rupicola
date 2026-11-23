//! `/selections/{id}`: one frozen selection.

use crate::{selections, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new().route("/{id}", routing::get(selections::get))
}
