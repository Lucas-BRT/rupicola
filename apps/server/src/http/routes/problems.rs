//! `/problems/{id}`: one problem, its comments and the selections it cites.

use super::super::handlers::problems;
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/{id}",
            routing::get(problems::get)
                .patch(problems::update)
                .delete(problems::delete),
        )
        .route("/{id}/comments", routing::post(problems::comment))
        .route(
            "/{id}/selections/{selection}",
            routing::put(problems::cite_selection).delete(problems::uncite_selection),
        )
}
