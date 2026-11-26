//! `GET` and `POST /projects/{project}/captures`

use super::{current, project, tab_metadata};
use crate::app::{account, fail, panel, respond};
use crate::components::skeleton::captures_tab_skeleton;
use crate::views::project::Tab;
use crate::{
    PageResult, StatusCode, auth, auth::Role, project::Filter, records, redirect,
    views::project::captures_tab,
};
use rupicola_api::{NewTopic, TopicMove, TopicRename};
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
    #[serde(default)]
    action: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    topic_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    parent_id: String,
    #[serde(default)]
    generation: i64,
    #[serde(default)]
    recursive: Option<String>,
    #[serde(default)]
    capture_id: String,
}

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Captures).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    Ok(show(cx, parse_query_params(cx)?))
}

#[route(POST)]
async fn post(cx: &Cx, Form(form): Form<TopicForm>) -> Result<Response> {
    respond(cx, handle_topic_action(cx, form).await)
}

async fn handle_topic_action(cx: &Cx, form: TopicForm) -> PageResult<Response> {
    let project_id = project(cx);
    let (p, acc) = (panel(cx), account(cx));
    auth::check_csrf(&acc, &form.csrf)?;
    if auth::role_of(cx, project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let base_captures = format!("/projects/{project_id}/captures");
    match form.action.as_str() {
        "rename" => {
            let tid = form.topic_id.trim();
            let name = form.name.trim();
            if tid.is_empty() || name.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            p.api
                .rename_topic(
                    &acc.session,
                    tid,
                    &TopicRename {
                        name: name.to_owned(),
                        generation: form.generation,
                    },
                )
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&format!("{base_captures}?topic={tid}")))
        }
        "move" => {
            let tid = form.topic_id.trim();
            if tid.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let parent_id = Some(form.parent_id.trim().to_owned()).filter(|s| !s.is_empty());
            p.api
                .move_topic(
                    &acc.session,
                    tid,
                    &TopicMove {
                        parent_id,
                        generation: form.generation,
                    },
                )
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&format!("{base_captures}?topic={tid}")))
        }
        "delete" => {
            let tid = form.topic_id.trim();
            if tid.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let recursive = form.recursive.as_deref() == Some("true");
            p.api
                .delete_topic(&acc.session, tid, recursive)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&base_captures))
        }
        "unfile" => {
            let tid = form.topic_id.trim();
            let cid = form.capture_id.trim();
            if tid.is_empty() || cid.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            p.api
                .unfile_topic_capture(&acc.session, tid, cid)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&format!("{base_captures}?topic={tid}")))
        }
        _ => {
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
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&base_captures))
        }
    }
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
