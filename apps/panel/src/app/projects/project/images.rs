//! `GET /projects/{project}/images`: the captures tab with the images view.

use crate::app::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use topcoat::{
    Result,
    context::Cx,
    router::{parse_query_params, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    let mut filter: crate::project::Filter = parse_query_params(cx)?;
    filter.view.get_or_insert_with(|| "images".into());
    respond(
        cx,
        super::captures::show(panel(cx), account(cx), project_param(cx).into(), filter).await,
    )
}
