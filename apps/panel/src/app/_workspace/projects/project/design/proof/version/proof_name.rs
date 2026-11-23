//! `GET /projects/{project}/design/proof/{version}/{proof_name}`

use crate::app::{account, panel, respond};
use crate::{app::_workspace::projects::project::project as project_param, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{module_param, response::Response, route},
};

module_param!(pub proof_name);

#[route(GET)]
async fn proof(cx: &Cx) -> Result<Response> {
    let project_id: String = project_param(cx).into();
    let version_id: String = super::version(cx).into();
    let name: String = proof_name(cx).into();
    respond(
        cx,
        design_system::proof(&panel(cx), &account(cx), &project_id, &version_id, &name).await,
    )
}
