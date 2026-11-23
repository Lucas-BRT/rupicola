//! `GET /dashboard`

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{parse_query_params, response::Response, route},
};

#[route(GET)]
async fn dashboard(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::dashboard(panel(cx), account(cx), parse_query_params(cx)?).await,
    )
}
