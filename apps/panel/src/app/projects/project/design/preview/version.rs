//! `GET /projects/{project}/design/preview/{version}`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{module_param, path_param, response::Response, route},
};

module_param!(pub version);

#[route(GET)]
async fn preview(cx: &Cx) -> Result<Response> {
    let project_id: String = path_param::<Project>(cx).into();
    let version: String = path_param::<Version>(cx).into();
    respond(
        cx,
        design_system::preview(&panel(cx), &account(cx), &project_id, &version).await,
    )
}
