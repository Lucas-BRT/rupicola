//! `GET /projects/{project}/design/preview/{version}`: the example pages, for the frame.

use crate::app::{account, fail, panel};
use crate::{app::_workspace::projects::project::project as project_param, auth};
use http::header;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, module_param, response::Response, route},
};

module_param!(pub version);

/// What the examples may load and do inside the panel's frame: they run scripts of their
/// own, with no origin, no network and no way out.
const PREVIEW_POLICY: &str = "sandbox allow-scripts; default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; media-src data:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'";

#[route(GET)]
async fn preview(cx: &Cx) -> Result<Response> {
    let page = panel(cx)
        .api
        .design_preview(&account(cx).session, project_param(cx), version(cx))
        .await
        .map_err(|e| fail(auth::api_error(&e)))?;
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, PREVIEW_POLICY)
        .body(Body::from(page.bytes))
        .map_err(|e| fail(crate::db_error(e)))
}
