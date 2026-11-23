//! `POST /settings/invitation`

use crate::app::{account, panel, respond};
use crate::{Account, Arc, Deserialize, PageResult, Panel, StatusCode, auth, redirect};
use rupicola_api::Redeem;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn accept_invitation(cx: &Cx, Form(form): Form<Invitation>) -> Result<Response> {
    respond(cx, submit(panel(cx), account(cx), form).await)
}

#[derive(Deserialize)]
struct Invitation {
    csrf: String,
    code: String,
}

async fn submit(panel: Arc<Panel>, account: Account, form: Invitation) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if account.owner {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .redeem_invite(&account.session, &Redeem { code: form.code })
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect("/projects"))
}
