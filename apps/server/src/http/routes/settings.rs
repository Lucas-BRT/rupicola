//! `/settings`: the server-wide settings an administrator edits.

use super::super::handlers::settings;
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(settings::server))
        .route(
            "/{key}",
            routing::put(settings::set_server).delete(settings::reset_server),
        )
}
