//! `POST /settings/invitation`

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn accept_invitation(cx: &Cx, Form(form): Form<pages::Invitation>) -> Result<Response> {
    respond(
        cx,
        pages::accept_invitation(panel(cx), account(cx), form).await,
    )
}
