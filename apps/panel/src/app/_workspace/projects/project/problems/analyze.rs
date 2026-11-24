//! `POST /projects/{project}/problems/analyze`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{PageResult, StatusCode, auth, redirect, safe_name};
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
    if !safe_name(&project_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    panel
        .api
        .analyze_capture(&account.session, &project_id, &form.run_key)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}
