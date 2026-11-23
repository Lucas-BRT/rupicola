//! `GET /projects/{project}/design/proof/{version}/{proof_name}`: one validation screenshot.

use crate::app::{account, fail, panel};
use crate::{app::_workspace::projects::project::project as project_param, auth};
use http::header;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, module_param, response::Response, route},
};

module_param!(pub proof_name);

#[route(GET)]
async fn proof(cx: &Cx) -> Result<Response> {
    let shot = panel(cx)
        .api
        .design_proof(
            &account(cx).session,
            project_param(cx),
            super::version(cx),
            proof_name(cx),
        )
        .await
        .map_err(|e| fail(auth::api_error(&e)))?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(shot.bytes))
        .map_err(|e| fail(crate::db_error(e)))
}
