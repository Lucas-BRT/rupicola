//! `GET /settings` and `POST /settings`

mod invitation;
mod password;

use crate::app::{_workspace::title, account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, auth, redirect, views::settings::settings_page};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, page, response::Response, route},
    view::{View, view},
};

#[page(with(Metadata::new().title(title("Settings"))))]
async fn page(cx: &Cx) -> Result<impl View> {
    Ok(view! { settings_page() })
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
