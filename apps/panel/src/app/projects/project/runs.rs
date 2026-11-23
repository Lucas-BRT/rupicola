//! `GET /projects/{project}/runs`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, pages};
use topcoat::{
    Result,
    context::Cx,
    router::{parse_query_params, path_param, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::project_section(
            panel(cx),
            account(cx),
            (path_param::<Project>(cx).into(), "runs".into()),
            parse_query_params(cx)?,
        )
        .await,
    )
}
