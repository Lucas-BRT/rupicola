//! `GET` and `POST /run/{capture_id}`: one capture and its review.

use crate::app::{_workspace::title, account, fail, panel, respond};
use crate::{
    Account, Deserialize, PageResult, Panel, Run, StatusCode, auth,
    auth::Role,
    media, redirect, runs,
    views::run::{Capture, run_page},
};
use rupicola_api::{CaptureStatus, NewFinding};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, module_param, page, parse_query_params, response::Response, route},
    view::{View, view},
};

module_param!(pub capture_id);

#[derive(Deserialize)]
struct Comparison {
    before: Option<String>,
}

#[page(with(Metadata::from_fn(async |cx| {
    let run = runs::find(cx, capture_id(cx)).await.map_err(fail)?;
    Ok(Metadata::new().title(title(&run.scenario)))
})))]
async fn page(cx: &Cx) -> Result<impl View> {
    let comparison: Comparison = parse_query_params(cx)?;
    let data = load(cx, comparison, capture_id(cx).to_owned())
        .await
        .map_err(fail)?;
    Ok(view! { run_page(data: &data) })
}

async fn load(cx: &Cx, comparison: Comparison, id: String) -> PageResult<Capture> {
    let found = runs::find(cx, &id).await?;
    let key = found.key;
    let project_label = found.project;
    let run = found.capture;
    let role = auth::role_of(cx, &run.project_id).await?;
    let candidates: Vec<_> = runs::summaries(cx, &run.project_id)
        .await?
        .iter()
        .filter(|entry| {
            entry.scenario == run.scenario
                && entry.status == CaptureStatus::Complete
                && entry.id != key
        })
        .cloned()
        .collect();
    let requested = comparison.before;
    let before = match requested
        .as_deref()
        .filter(|before| candidates.iter().any(|candidate| candidate.id == *before))
    {
        Some(before) => {
            let before = runs::find(cx, before).await?.capture;
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
                .map(|item| (item.id.clone(), media::url(&item.artifact.id)))
        })
        .collect();
    Ok(Capture {
        width: run.environment.width.unwrap_or(640).max(1).to_string(),
        height: run.environment.height.unwrap_or(480).max(1).to_string(),
        may_review: role != Role::Reader,
        comparing: before.is_some(),
        project_url: format!("/projects/{}", run.project_id),
        project_label,
        form_url: format!("/run/{key}"),
        record: run,
        candidates,
        requested,
        before_images,
        environments_differ,
    })
}

#[route(POST)]
async fn inspect(cx: &Cx, Form(form): Form<Review>) -> Result<Response> {
    respond(cx, review(cx, capture_id(cx).into(), form).await)
}

#[derive(Deserialize)]
struct Review {
    csrf: String,
    note: String,
    evidence: Option<String>,
}
async fn review(cx: &Cx, id: String, form: Review) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    let run = runs::find(cx, &id).await?;
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
            .is_some_and(|artifact| !media::declared(&run.capture, artifact))
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
