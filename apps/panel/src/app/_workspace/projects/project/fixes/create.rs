//! `POST /projects/{project}/fixes/create`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{Account, Arc, PageResult, Panel, StatusCode, auth, redirect, safe_name};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<CreateFixForm>) -> Result<Response> {
    respond(
        cx,
        create_fix_action(panel(cx), account(cx), project_param(cx).into(), form).await,
    )
}

/// Verification recorded for a step written without one.
const DEFAULT_VERIFICATION: &str = "Verified visually";

#[derive(Deserialize)]
pub struct CreateFixForm {
    pub csrf: String,
    pub title: String,
    pub problem_id: Option<String>,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub before_url: String,
    pub after_url: String,
    pub tactics: String,
    pub files: String,
    pub summary: String,
    pub steps: String,
}

pub async fn create_fix_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: CreateFixForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&project_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let title = form.title.trim();
    let summary = form.summary.trim();
    if title.is_empty() || summary.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let problems: Vec<String> = form
        .problem_id
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let list = |text: &str| -> Vec<String> {
        text.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let steps: Vec<rupicola_api::NewStep> = form
        .steps
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, "::").collect();
            let (st_title, detail, verification) = match parts.as_slice() {
                [a, b, c] => (a.trim(), b.trim(), c.trim()),
                [a, b] => (a.trim(), b.trim(), DEFAULT_VERIFICATION),
                _ => (line, "", DEFAULT_VERIFICATION),
            };
            rupicola_api::NewStep {
                title: st_title.into(),
                detail: detail.into(),
                verification: verification.into(),
                evidence: None,
            }
        })
        .collect();
    let route = form.route.trim().to_string();
    let viewport = form.viewport.trim().to_string();
    let theme = form.theme.trim().to_string();
    let (before, after) = (form.before_url.trim(), form.after_url.trim());
    let comparisons = if before.is_empty() || after.is_empty() {
        Vec::new()
    } else {
        vec![rupicola_api::Comparison {
            label: format!("{route} · {viewport} · {theme}"),
            route,
            viewport,
            theme,
            before: crate::records::artifact_of(before).to_owned(),
            after: crate::records::artifact_of(after).to_owned(),
            diff: None,
            changed_ratio: None,
        }]
    };
    let new = rupicola_api::NewFix {
        title: title.into(),
        summary: summary.into(),
        problems,
        tactics: list(&form.tactics),
        files: list(&form.files),
        comparisons,
        steps,
        baseline: Vec::new(),
        proof: Vec::new(),
    };
    let fix = panel
        .api
        .create_fix(&account.session, &project_id, &new)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!(
        "/projects/{project_id}/fixes#fix-{}",
        fix.id
    )))
}
