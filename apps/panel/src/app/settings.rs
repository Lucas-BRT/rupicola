//! `GET /settings` and `POST /settings`

mod invitation;
mod password;

use crate::app::{account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, auth, redirect, views};
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Html},
        response::Response,
        route,
    },
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(cx, show(panel(cx), account(cx)).await)
}

async fn show(panel: Arc<Panel>, account: Account) -> PageResult<Html<String>> {
    let cx = &Cx::default();
    views::settings::render(cx, &account, &panel).await
}

/// Ends the account's other sessions.
#[route(POST)]
async fn sign_out_others(cx: &Cx, Form(form): Form<auth::Csrf>) -> Result<Response> {
    respond(cx, end_other_sessions(panel(cx), account(cx), form).await)
}

async fn end_other_sessions(
    panel: Arc<Panel>,
    account: Account,
    form: auth::Csrf,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    auth::sign_out_others(&panel, &account).await?;
    Ok(redirect("/settings"))
}
