//! `/settings`: the server-wide settings an administrator edits.

use crate::{settings, state::Shared};
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(settings::server))
        .route(
            "/{key}",
            routing::put(settings::set_server).delete(settings::reset_server),
        )
}
