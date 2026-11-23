//! `POST /projects/{project}/problems/analyze`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{PageResult, StatusCode, auth, auth::Role, redirect};
use rupicola_core::problem_record;
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
    match analyze_run(cx, &project_id, &form.run_key).await {
        Ok(()) | Err(StatusCode::NOT_FOUND) => {}
        Err(error) => return Err(error),
    }
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}

pub(crate) async fn analyze_run(cx: &Cx, project_id: &str, run_key: &str) -> PageResult<()> {
    let (panel, account) = (panel(cx), account(cx));
    let (panel, account) = (&panel, &account);
    let existing = crate::records::problems(cx, project_id, &[], &[]).await?;
    let manifest = crate::runs::find(cx, run_key)
        .await
        .map(|run| run.manifest)
        .ok()
        .filter(|manifest| manifest.project.id == project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if manifest.findings.is_empty() {
        if let Some(first) = manifest.checkpoints.first() {
            let candidate = problem_record::infer_from_finding(
                project_id,
                run_key,
                &format!(
                    "[{} @ {}×{} {}] Visual inspection requested on capture '{}'",
                    first.id,
                    manifest.environment.width.unwrap_or(1440),
                    manifest.environment.height.unwrap_or(900),
                    manifest.environment.color_scheme,
                    manifest.scenario
                ),
                Some(&first.path),
            );
            if !existing
                .iter()
                .any(|problem| problem.diagnosis == candidate.diagnosis)
            {
                crate::records::open_inferred(
                    panel,
                    account,
                    &candidate,
                    Some(rupicola_api::ProblemStatus::NeedsReprocess),
                )
                .await?;
            }
        }
    } else {
        for f in &manifest.findings {
            let candidate = problem_record::infer_from_finding(
                project_id,
                run_key,
                &f.note,
                f.evidence.as_deref(),
            );
            let rule_code = candidate
                .violated_rules
                .first()
                .map_or("", |r| r.code.as_str());
            if problem_record::is_suppressed(
                &existing,
                &candidate.route,
                rule_code,
                &candidate.title,
            ) {
                continue;
            }
            if !existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                crate::records::open_inferred(panel, account, &candidate, None).await?;
            }
        }
    }
    Ok(())
}
