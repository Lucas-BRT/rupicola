//! `GET /register` and `POST /register`

use crate::app::{panel, respond};
use crate::auth;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, request, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(cx, auth::register_page(panel(cx)).await)
}

#[route(POST)]
async fn sign_up(cx: &Cx, Form(form): Form<auth::Credentials>) -> Result<Response> {
    respond(
        cx,
        auth::register(panel(cx), request::headers(cx).clone(), form).await,
    )
}
