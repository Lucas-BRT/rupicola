//! `POST /projects/{project}/fixes/{fix}/review`

use crate::app::{account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, StatusCode, auth, auth::Role, redirect};
use crate::{app::projects::project::Project, app::projects::project::fixes::fix::Fix};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn submit(cx: &Cx, Form(form): Form<ReviewFixForm>) -> Result<Response> {
    respond(
        cx,
        review_fix_action(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Fix>(cx).into(),
            ),
            form,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub struct ReviewFixForm {
    #[serde(default)]
    pub csrf: String,
    pub status: String,
    pub comment: Option<String>,
}

pub async fn review_fix_action(
    panel: Arc<Panel>,
    account: Account,
    (project_id, fix_id): (String, String),
    form: ReviewFixForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let outcome = form
        .status
        .trim()
        .parse()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let review = rupicola_api::NewReview {
        outcome,
        comment: form.comment.unwrap_or_default().trim().to_owned(),
    };
    panel
        .api
        .review_fix(&account.session, &fix_id, &review)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!(
        "/projects/{project_id}/fixes#fix-{fix_id}"
    )))
}
