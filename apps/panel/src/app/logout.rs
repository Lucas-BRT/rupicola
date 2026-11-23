//! `POST /logout`

use crate::app::{account, panel, respond};
use crate::auth;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn logout(cx: &Cx, Form(form): Form<auth::Csrf>) -> Result<Response> {
    respond(cx, auth::logout(panel(cx), account(cx), form).await)
}
