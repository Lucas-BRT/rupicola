//! `POST /projects/{project}/problems/analyze`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{PageResult, StatusCode, auth, auth::Role, redirect};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn analyze(cx: &Cx, Form(form): Form<AnalyzeRunForm>) -> Result<Response> {
    respond(
        cx,
        analyze_run_action(cx, project_param(cx).into(), form).await,
    )
}

#[derive(Deserialize)]
pub struct AnalyzeRunForm {
    pub csrf: String,
    pub run_key: String,
}

pub async fn analyze_run_action(
    cx: &Cx,
    project_id: String,
    form: AnalyzeRunForm,
) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    match panel
        .api
        .analyze_capture(&account.session, &project_id, &form.run_key)
        .await
    {
        Ok(_) => {}
        Err(error) => match auth::api_error(&error) {
            StatusCode::NOT_FOUND => {}
            status => return Err(status),
        },
    }
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}
