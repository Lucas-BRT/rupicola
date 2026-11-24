//! `GET /healthz`

use topcoat::{Result, context::Cx, router::route};

#[route(GET)]
async fn healthz(cx: &Cx) -> Result<&'static str> {
    Ok("ok")
}
