//! `GET` and `POST /projects/{project}/captures`

use super::{current, project, tab_metadata};
use crate::app::{account, fail, panel, respond};
use crate::components::skeleton::captures_tab_skeleton;
use crate::views::project::Tab;
use crate::{
    PageResult, StatusCode, auth, auth::Role, project::Filter, records, redirect,
    views::project::captures_tab,
};
use rupicola_api::NewTopic;
use serde::Deserialize;
use std::collections::BTreeSet;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, page, parse_query_params, response::Response, route},
    view::{View, component, error_boundary, suspense, view},
};

#[derive(Deserialize)]
struct TopicForm {
    csrf: String,
    path: String,
}

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Captures).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    Ok(show(cx, parse_query_params(cx)?))
}

#[route(POST)]
async fn post(cx: &Cx, Form(form): Form<TopicForm>) -> Result<Response> {
    respond(cx, create_topic(cx, form).await)
}

async fn create_topic(cx: &Cx, form: TopicForm) -> PageResult<Response> {
    let project_id = project(cx);
    let (p, acc) = (panel(cx), account(cx));
    auth::check_csrf(&acc, &form.csrf)?;
    if auth::role_of(cx, project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let path = form.path.trim();
    if path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    p.api
        .create_topic(
            &acc.session,
            project_id,
            &NewTopic {
                path: path.to_owned(),
            },
        )
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(redirect(&format!("/projects/{project_id}/captures")))
}

/// The captures tab, which `runs`, `images` and `videos` also open with another view.
pub(super) fn show(cx: &Cx, filter: Filter) -> impl View {
    view! {
        suspense(
            fallback: view! { captures_tab_skeleton() },
            error_boundary(
                fallback: |_| Ok(view! {
                    <p class="notice" role="alert">"Captures could not be loaded. Try again in a moment."</p>
                }),
                loaded_captures(filter: filter)
            )
        )
    }
}

#[component]
async fn loaded_captures(cx: &Cx, filter: Filter) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let assets = project.assets();
    let problems = records::problems(cx, &project.id, &assets, &[])
        .await
        .map_err(fail)?;
    let (p, acc) = (panel(cx), account(cx));
    let topics = p
        .api
        .topic_tree(&acc.session, &project.id)
        .await
        .map(|t| t.topics)
        .unwrap_or_default();
    let topic_run_ids = if let Some(tid) = filter.topic.as_deref().filter(|s| !s.is_empty()) {
        let captures = p
            .api
            .topic_captures(&acc.session, tid, false)
            .await
            .unwrap_or_default();
        Some(
            captures
                .into_iter()
                .map(|c| c.id)
                .collect::<BTreeSet<String>>(),
        )
    } else {
        None
    };
    Ok(view! {
        captures_tab(
            role: project.role,
            base: &project.base,
            filter: filter,
            runs: project.related(),
            assets: assets,
            problems: problems,
            topics: topics,
            topic_run_ids: topic_run_ids,
        )
    })
}
