//! `GET /architecture`

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
async fn architecture(cx: &Cx) -> TopcoatResult<Response> {
    Html(views::architecture(&cfg(cx).panel_url)).into_response(cx)
}
