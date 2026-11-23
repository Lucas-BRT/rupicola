//! `GET /healthz`

use http::{StatusCode, header};
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::{Body, response::Response, route},
};

#[route(GET)]
async fn healthz(_cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("ok"))
        .expect("valid healthz response"))
}
