//! `GET /install`

use super::cfg;
use crate::views;
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::{
        content::Html,
        response::{IntoResponse, Response},
        route,
    },
};

#[route(GET)]
async fn install(cx: &Cx) -> TopcoatResult<Response> {
    let config = cfg(cx);
    Html(views::install(&config.panel_url, &config.public_url)).into_response(cx)
}
