//! `POST /projects/{project}/problems/{problem}/triage`

use crate::app::{account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, StatusCode, auth, auth::Role, redirect};
use crate::{
    app::projects::project::problems::problem::problem as problem_param,
    app::projects::project::project as project_param,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn triage(cx: &Cx, Form(form): Form<TriageProblemForm>) -> Result<Response> {
    respond(
        cx,
        triage_problem_action(
            panel(cx),
            account(cx),
            (project_param(cx).into(), problem_param(cx).into()),
            form,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub struct TriageProblemForm {
    #[serde(default)]
    pub csrf: String,
    pub status: String,
    pub comment: Option<String>,
    pub solution: Option<String>,
}

pub async fn triage_problem_action(
    panel: Arc<Panel>,
    account: Account,
    (project_id, problem_id): (String, String),
    form: TriageProblemForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let status = form
        .status
        .trim()
        .parse()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let nonblank = |text: Option<String>| {
        text.map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let patch = rupicola_api::ProblemPatch {
        status: Some(status),
        severity: None,
        proposed_solution: nonblank(form.solution),
    };
    panel
        .api
        .update_problem(&account.session, &problem_id, &patch)
        .await
        .map_err(|e| auth::api_error(&e))?;
    if let Some(text) = nonblank(form.comment) {
        panel
            .api
            .comment_problem(
                &account.session,
                &problem_id,
                &rupicola_api::NewComment { text },
            )
            .await
            .map_err(|e| auth::api_error(&e))?;
    }
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
}
