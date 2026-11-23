//! `GET /projects/{project}/videos`: the captures tab with the videos view.

use crate::app::projects::project::Project;
use crate::app::{account, panel, respond};
use topcoat::{
    Result,
    context::Cx,
    router::{parse_query_params, path_param, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    let mut filter: crate::project::Filter = parse_query_params(cx)?;
    filter.view.get_or_insert_with(|| "videos".into());
    respond(
        cx,
        super::captures::show(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            filter,
        )
        .await,
    )
}
