//! `GET /projects/{project}/design/preview/{version}`

use crate::app::{account, panel, respond};
use crate::{app::_workspace::projects::project::project as project_param, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{module_param, response::Response, route},
};

module_param!(pub version);

#[route(GET)]
async fn preview(cx: &Cx) -> Result<Response> {
    let project_id: String = project_param(cx).into();
    let version_id: String = version(cx).into();
    respond(
        cx,
        design_system::preview(&panel(cx), &account(cx), &project_id, &version_id).await,
    )
}
