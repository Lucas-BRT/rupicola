//! `POST /projects/{project}/problems/create`

use crate::app::projects::project::Project;
use crate::app::{account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, StatusCode, auth, auth::Role, redirect};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<CreateProblemForm>) -> Result<Response> {
    respond(
        cx,
        create_problem_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub struct CreateProblemForm {
    pub csrf: String,
    pub title: String,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub severity: String,
    pub evidence_url: String,
    pub global_rule: String,
    pub project_rule: String,
    pub diagnosis: String,
    pub solution: String,
}

pub async fn create_problem_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: CreateProblemForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let title = form.title.trim();
    let diagnosis = form.diagnosis.trim();
    let solution = form.solution.trim();
    if title.is_empty() || diagnosis.is_empty() || solution.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let severity = form
        .severity
        .trim()
        .parse()
        .unwrap_or(rupicola_api::Severity::Major);
    let mut rules = Vec::new();
    for (scope, raw) in [
        ("global", &form.global_rule),
        ("project", &form.project_rule),
    ] {
        if !raw.trim().is_empty() {
            let (code, rule_title) = raw.split_once(':').unwrap_or((raw.as_str(), raw.as_str()));
            rules.push(rupicola_api::Rule {
                scope: scope.into(),
                code: code.trim().into(),
                title: rule_title.trim().into(),
            });
        }
    }
    let evidence: Vec<String> = Some(form.evidence_url.trim())
        .filter(|url| !url.is_empty())
        .map(|url| crate::records::artifact_of(url).to_owned())
        .into_iter()
        .collect();
    let new = rupicola_api::NewProblem {
        title: title.into(),
        severity,
        origin: rupicola_api::ProblemOrigin::UserReported,
        route: form.route.trim().into(),
        viewport: form.viewport.trim().into(),
        theme: form.theme.trim().into(),
        diagnosis: diagnosis.into(),
        proposed_solution: solution.into(),
        rules,
        evidence,
    };
    panel
        .api
        .create_problem(&account.session, &project_id, &new)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}
