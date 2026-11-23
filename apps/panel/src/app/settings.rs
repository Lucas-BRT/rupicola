//! `GET /settings` and `POST /settings`

mod invitation;
mod password;

use crate::app::{account, panel, respond};
use crate::{auth, pages};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(cx, pages::settings(panel(cx), account(cx)).await)
}

#[route(POST)]
async fn account_action(cx: &Cx, Form(form): Form<auth::Csrf>) -> Result<Response> {
    respond(
        cx,
        pages::account_action(panel(cx), account(cx), form).await,
    )
}
