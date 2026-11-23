//! `GET /projects/{project}/findings`: the problems tab.

use crate::app::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use topcoat::{
    Result,
    context::Cx,
    router::{parse_query_params, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        super::problems::show(
            panel(cx),
            account(cx),
            project_param(cx).into(),
            parse_query_params(cx)?,
        )
        .await,
    )
}
