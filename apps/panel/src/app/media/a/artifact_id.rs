//! `GET` and `HEAD /media/a/{artifact_id}`: an artifact's bytes, with ranges.

use crate::app::{account, panel, respond};
use crate::media;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, module_param, path_param, request, request::Request, response::Response, route,
    },
};

module_param!(pub artifact_id);

#[route([GET, HEAD])]
async fn artifact(cx: &Cx) -> Result<Response> {
    let mut request = Request::new(Body::empty());
    *request.method_mut() = request::method(cx).clone();
    *request.headers_mut() = request::headers(cx).clone();
    respond(
        cx,
        media::by_id(
            account(cx),
            panel(cx),
            path_param::<ArtifactId>(cx).into(),
            request,
        )
        .await,
    )
}
