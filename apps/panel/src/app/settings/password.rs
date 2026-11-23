//! `POST /settings/password`

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn change_password(cx: &Cx, Form(form): Form<pages::PasswordChangeForm>) -> Result<Response> {
    respond(
        cx,
        pages::change_password(panel(cx), account(cx), form).await,
    )
}
