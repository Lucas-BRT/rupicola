//! `GET /projects/{project}/design/proof/{version}/{proof_name}`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{module_param, path_param, response::Response, route},
};

module_param!(pub proof_name);

#[route(GET)]
async fn proof(cx: &Cx) -> Result<Response> {
    let project_id: String = path_param::<Project>(cx).into();
    let version: String = path_param::<super::Version>(cx).into();
    let name: String = path_param::<ProofName>(cx).into();
    respond(
        cx,
        design_system::proof(&panel(cx), &account(cx), &project_id, &version, &name).await,
    )
}
