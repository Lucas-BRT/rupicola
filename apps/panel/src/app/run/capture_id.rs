//! `GET` and `POST /run/{capture_id}`: one capture and its review.

use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, Deserialize, PageResult, Panel, Run, StatusCode, auth,
    auth::Role,
    media, redirect, runs,
    views::{self, run::Capture as RunView},
};
use rupicola_api::{CaptureStatus, NewFinding};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Html},
        module_param, parse_query_params,
        response::Response,
        route,
    },
};

module_param!(pub capture_id);

#[derive(Deserialize)]
struct Comparison {
    before: Option<String>,
}

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(
            parse_query_params(cx)?,
            account(cx),
            panel(cx),
            capture_id(cx).into(),
        )
        .await,
    )
}

async fn show(
    comparison: Comparison,
    account: Account,
    panel: Arc<Panel>,
    id: String,
) -> PageResult<Html<String>> {
    let found = runs::find(&panel, &account, &id).await?;
    let key = found.key;
    let run = found.manifest;
    let role = auth::role(&panel, &account, &run.project.id).await?;
    let candidates: Vec<_> = runs::summaries(&panel, &account, &run.project.id)
        .await?
        .into_iter()
        .filter(|entry| {
            entry.scenario == run.scenario
                && entry.status == CaptureStatus::Complete
                && entry.id != key
        })
        .collect();
    let requested = comparison.before;
    let before = match requested
        .as_deref()
        .filter(|before| candidates.iter().any(|candidate| candidate.id == *before))
    {
        Some(before) => {
            let before = runs::find(&panel, &account, before).await?.manifest;
            let differs = before.environment != run.environment;
            Some((before.checkpoints, differs))
        }
        None => None,
    };
    let environments_differ = before.as_ref().is_some_and(|(_, differs)| *differs);
    let before_images: BTreeMap<_, _> = before
        .as_ref()
        .into_iter()
        .flat_map(|(checkpoints, _)| {
            checkpoints
                .iter()
                .map(|item| (item.id.clone(), media::url(&item.path)))
        })
        .collect();
    let width = run.environment.width.unwrap_or(640).max(1).to_string();
    let height = run.environment.height.unwrap_or(480).max(1).to_string();
    let cx = &Cx::default();
    let may_review = role != Role::Reader;
    let comparing = before.is_some();
    let project_url = format!("/projects/{}", run.project.id);
    let form_url = format!("/run/{key}");
    let data = RunView {
        manifest: &run,
        candidates: &candidates,
        requested,
        before_images: &before_images,
        environments_differ,
        comparing,
        may_review,
        width: &width,
        height: &height,
        project_url: &project_url,
        form_url: &form_url,
    };
    views::run::render(cx, &account, &panel, data).await
}

#[route(POST)]
async fn inspect(cx: &Cx, Form(form): Form<Review>) -> Result<Response> {
    respond(
        cx,
        review(panel(cx), account(cx), capture_id(cx).into(), form).await,
    )
}

#[derive(Deserialize)]
struct Review {
    csrf: String,
    note: String,
    evidence: Option<String>,
}
async fn review(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: Review,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    let run = runs::find(&panel, &account, &id).await?;
    if auth::role(&panel, &account, &run.project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    record_inspection(&panel, &account, &run, &form.note, form.evidence.as_deref()).await?;
    Ok(redirect(&format!("/run/{id}#review")))
}

/// Records a review note on a capture; the evidence is the id of one of its artifacts.
async fn record_inspection(
    panel: &Panel,
    account: &Account,
    run: &Run,
    note: &str,
    evidence: Option<&str>,
) -> PageResult<()> {
    let note = note.trim();
    let evidence = evidence.filter(|artifact| !artifact.is_empty());
    if note.is_empty()
        || note.len() > 8000
        || evidence
            .as_ref()
            .is_some_and(|artifact| !media::declared(&run.manifest, artifact))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .add_finding(
            &account.session,
            &run.key,
            &NewFinding {
                note: note.to_owned(),
                evidence_artifact_id: evidence.map(str::to_owned),
            },
        )
        .await
        .map(|_| ())
        .map_err(|e| auth::api_error(&e))
}
