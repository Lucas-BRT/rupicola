//! `POST /projects/{project}/captures/run`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{PageResult, StatusCode, auth, redirect, safe_name};
use rupicola_api::{Environment, RunCapture};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn run(cx: &Cx, Form(form): Form<RunCaptureForm>) -> Result<Response> {
    respond(
        cx,
        run_capture_action(cx, project_param(cx).into(), form).await,
    )
}

#[derive(Deserialize)]
pub struct RunCaptureForm {
    pub csrf: String,
    pub scenario: String,
    pub url: String,
    pub width: Option<String>,
    pub height: Option<String>,
    pub color_scheme: Option<String>,
}

fn parse_optional_u32(value: Option<&str>) -> PageResult<Option<u32>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => raw
            .parse::<u32>()
            .map(Some)
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY),
    }
}

pub async fn run_capture_action(
    cx: &Cx,
    project_id: String,
    form: RunCaptureForm,
) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&project_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let width = parse_optional_u32(form.width.as_deref())?;
    let height = parse_optional_u32(form.height.as_deref())?;
    let capture = panel
        .api
        .run_capture(
            &account.session,
            &project_id,
            &RunCapture {
                scenario: form.scenario.trim().to_owned(),
                url: form.url.trim().to_owned(),
                revision: None,
                environment: Environment {
                    width,
                    height,
                    color_scheme: form
                        .color_scheme
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty()),
                    ..Environment::default()
                },
                timeout_ms: None,
                steps: Vec::new(),
                allow_degraded: false,
            },
        )
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!("/run/{}", capture.id)))
}
