//! `/fixes/{id}`: one fix, its steps, its reviews and the selections it cites.

use crate::{fixes, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/{id}", routing::get(fixes::get))
        .route("/{id}/steps", routing::post(fixes::add_step))
        .route("/{id}/reviews", routing::post(fixes::review))
        .route(
            "/{id}/selections/{selection}",
            routing::put(fixes::cite_selection).delete(fixes::uncite_selection),
        )
}
