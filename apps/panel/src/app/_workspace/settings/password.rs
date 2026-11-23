//! `POST /settings/password`

use crate::app::{account, panel, respond};
use crate::{Account, Arc, Deserialize, PageResult, Panel, StatusCode, auth, redirect};
use rupicola_api::PasswordChange;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn change_password(cx: &Cx, Form(form): Form<PasswordChangeForm>) -> Result<Response> {
    respond(cx, submit(panel(cx), account(cx), form).await)
}

#[derive(Deserialize)]
struct PasswordChangeForm {
    csrf: String,
    current_password: String,
    new_password: String,
    confirm_password: String,
}

async fn submit(
    panel: Arc<Panel>,
    account: Account,
    form: PasswordChangeForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !auth::equal(&form.new_password, &form.confirm_password) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let change = PasswordChange {
        current: form.current_password,
        new: form.new_password,
    };
    auth::change_password(&panel, &account, &change).await?;
    Ok(redirect("/settings"))
}
