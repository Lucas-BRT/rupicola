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
    let role = auth::role_of(cx, project_id).await?;
    execute_topic_action(&p, &acc, role, project_id, form).await
}

async fn execute_topic_action(
    p: &crate::Panel,
    acc: &crate::Account,
    role: Role,
    project_id: &str,
    form: TopicForm,
) -> PageResult<Response> {
    auth::check_csrf(acc, &form.csrf)?;
    if role == Role::Reader {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, Panel};
    use rupicola_api::Topic;
    use rupicola_client::MockApi;

    fn test_account() -> Account {
        Account {
            handle: "rev".to_owned(),
            owner: false,
            session: "sess-1".to_owned(),
            csrf: "csrf-ok".to_owned(),
        }
    }

    fn base_form(action: &str) -> TopicForm {
        TopicForm {
            csrf: "csrf-ok".to_owned(),
            action: action.to_owned(),
            path: String::new(),
            topic_id: String::new(),
            name: String::new(),
            parent_id: String::new(),
            generation: 0,
            recursive: None,
            capture_id: String::new(),
        }
    }

    fn sample_topic(id: &str, path: &str) -> Topic {
        Topic {
            id: id.to_owned(),
            parent_id: None,
            name: path.to_owned(),
            path: path.to_owned(),
            position: 0,
            generation: 1,
            captures: 0,
        }
    }

    #[tokio::test]
    async fn topic_actions_enforce_csrf_and_reader_role() {
        let panel = Panel::for_test(MockApi::new());
        let acc = test_account();

        let mut bad_csrf = base_form("create");
        bad_csrf.csrf = "bad".to_owned();
        assert_eq!(
            execute_topic_action(&panel, &acc, Role::Reviewer, "demo", bad_csrf)
                .await
                .unwrap_err(),
            StatusCode::FORBIDDEN
        );

        let mut as_reader = base_form("create");
        as_reader.path = "auth/login".to_owned();
        assert_eq!(
            execute_topic_action(&panel, &acc, Role::Reader, "demo", as_reader)
                .await
                .unwrap_err(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn topic_crud_and_unfile_forward_expected_payloads() {
        let mut api = MockApi::new();
        api.expect_create_topic()
            .times(1)
            .withf(|tok, proj, form| tok == "sess-1" && proj == "demo" && form.path == "auth/login")
            .returning(|_, _, _| Ok(sample_topic("t1", "auth/login")));
        api.expect_rename_topic()
            .times(1)
            .withf(|_, id, form| id == "t1" && form.name == "signin" && form.generation == 2)
            .returning(|_, _, _| Ok(sample_topic("t1", "auth/signin")));
        api.expect_move_topic()
            .times(1)
            .withf(|_, id, form| id == "t1" && form.parent_id.is_none() && form.generation == 3)
            .returning(|_, _, _| Ok(sample_topic("t1", "signin")));
        api.expect_delete_topic()
            .times(1)
            .withf(|_, id, rec| id == "t1" && *rec)
            .returning(|_, _, _| Ok(()));
        api.expect_unfile_topic_capture()
            .times(1)
            .withf(|_, tid, cid| tid == "t1" && cid == "cap-7")
            .returning(|_, _, _| Ok(()));

        let panel = Panel::for_test(api);
        let acc = test_account();

        let mut create = base_form("");
        create.path = "  auth/login  ".to_owned();
        let r = execute_topic_action(&panel, &acc, Role::Reviewer, "demo", create)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SEE_OTHER);

        let mut rename = base_form("rename");
        rename.topic_id = "t1".to_owned();
        rename.name = "signin".to_owned();
        rename.generation = 2;
        let r = execute_topic_action(&panel, &acc, Role::Reviewer, "demo", rename)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SEE_OTHER);

        let mut mv = base_form("move");
        mv.topic_id = "t1".to_owned();
        mv.parent_id = "  ".to_owned();
        mv.generation = 3;
        let r = execute_topic_action(&panel, &acc, Role::Reviewer, "demo", mv)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SEE_OTHER);

        let mut del = base_form("delete");
        del.topic_id = "t1".to_owned();
        del.recursive = Some("true".to_owned());
        let r = execute_topic_action(&panel, &acc, Role::Reviewer, "demo", del)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SEE_OTHER);

        let mut unfile = base_form("unfile");
        unfile.topic_id = "t1".to_owned();
        unfile.capture_id = "cap-7".to_owned();
        let r = execute_topic_action(&panel, &acc, Role::Reviewer, "demo", unfile)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SEE_OTHER);

        assert_eq!(
            execute_topic_action(&panel, &acc, Role::Reviewer, "demo", base_form("rename"))
                .await
                .unwrap_err(),
            StatusCode::BAD_REQUEST
        );
    }
}
