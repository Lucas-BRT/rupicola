//! `GET /projects/{project}`: the overview; each child module is a tab of the project.

mod captures;
mod design;
mod findings;
mod fixes;
mod images;
mod problems;
mod runs;
mod settings;
mod videos;

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{module_param, path_param, response::Response, route},
};

module_param!(pub project);

#[route(GET)]
async fn overview(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::project(panel(cx), account(cx), path_param::<Project>(cx).into()).await,
    )
}
