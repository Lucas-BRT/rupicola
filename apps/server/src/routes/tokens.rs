//! `/tokens`: the signed-in account's API tokens.

use crate::{credentials, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/",
            routing::get(credentials::list_tokens).post(credentials::create_token),
        )
        .route("/{id}", routing::delete(credentials::delete_token))
}
