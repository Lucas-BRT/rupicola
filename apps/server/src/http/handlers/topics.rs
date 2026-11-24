//! `/projects/{id}/topics` and `/topics/{id}`: a project's topic tree and what is filed in it.

use crate::{
    domain::{ApiError, Principal},
    services::topics,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rupicola_api::{CaptureSummary, NewTopic, Topic, TopicMove, TopicRename, TopicTree};
use serde::Deserialize;

/// `GET /v1/projects/{id}/topics`
pub async fn tree(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<TopicTree>, ApiError> {
    topics::tree(&state, &principal, &project).await.map(Json)
}

/// `POST /v1/projects/{id}/topics`: 201 when something was made, 200 when it all existed.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewTopic>,
) -> Result<(StatusCode, Json<Topic>), ApiError> {
    let (topic, made) = topics::create(&state, &principal, &project, &form).await?;
    let status = if made {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(topic)))
}

/// `PUT /v1/topics/{id}/name`
pub async fn rename(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Json(form): Json<TopicRename>,
) -> Result<Json<Topic>, ApiError> {
    topics::rename(&state, &principal, &topic, &form.name, form.generation)
        .await
        .map(Json)
}

/// `PUT /v1/topics/{id}/parent`
pub async fn move_to(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Json(form): Json<TopicMove>,
) -> Result<Json<Topic>, ApiError> {
    topics::move_to(&state, &principal, &topic, &form)
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct Removal {
    #[serde(default)]
    recursive: bool,
}

/// `DELETE /v1/topics/{id}`; `?recursive=true` takes the subtopics along.
pub async fn remove(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Query(removal): Query<Removal>,
) -> Result<StatusCode, ApiError> {
    topics::remove(&state, &principal, &topic, removal.recursive).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Listing {
    #[serde(default)]
    direct: bool,
}

/// `GET /v1/topics/{id}/captures`; `?direct=true` leaves out the subtopics.
pub async fn captures(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Query(listing): Query<Listing>,
) -> Result<Json<Vec<CaptureSummary>>, ApiError> {
    topics::captures(&state, &principal, &topic, listing.direct)
        .await
        .map(Json)
}

/// `PUT /v1/topics/{id}/captures/{capture}`
pub async fn file(
    State(state): State<Shared>,
    principal: Principal,
    Path((topic, capture)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    topics::file(&state, &principal, &topic, &capture).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/topics/{id}/captures/{capture}`
pub async fn unfile(
    State(state): State<Shared>,
    principal: Principal,
    Path((topic, capture)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    topics::unfile(&state, &principal, &topic, &capture).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{CaptureSummary, Topic, TopicTree};

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000000a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000000b1";

    /// Ana owns `demo` with captures A and B; Bruno reads; Carla owns `other`.
    async fn demo() -> (u16, [String; 3]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, carla] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        for (project, owner) in [("demo", &ana), ("other", &carla)] {
            call(
                port,
                "POST",
                "/v1/projects",
                Some(owner),
                Some(&format!(r#"{{"id":"{project}","label":"{project}"}}"#)),
            )
            .await;
        }
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        for id in [A, B] {
            publish_record(port, &ana, &capture_record(id, &sha, 6, Vec::new())).await;
        }
        (port, [ana, bruno, carla])
    }

    async fn make(port: u16, token: &str, project: &str, path: &str) -> String {
        call(
            port,
            "POST",
            &format!("/v1/projects/{project}/topics"),
            Some(token),
            Some(&serde_json::json!({ "path": path }).to_string()),
        )
        .await
    }

    async fn tree(port: u16, token: &str) -> Vec<Topic> {
        json::<TopicTree>(&call(port, "GET", "/v1/projects/demo/topics", Some(token), None).await)
            .topics
    }

    async fn ids_under(port: u16, token: &str, topic: &str, direct: bool) -> Vec<String> {
        let response = call(
            port,
            "GET",
            &format!("/v1/topics/{topic}/captures?direct={direct}"),
            Some(token),
            None,
        )
        .await;
        let mut ids: Vec<String> = json::<Vec<CaptureSummary>>(&response)
            .into_iter()
            .map(|capture| capture.id)
            .collect();
        ids.sort();
        ids
    }

    async fn file(port: u16, token: &str, topic: &str, capture: &str) -> String {
        call(
            port,
            "PUT",
            &format!("/v1/topics/{topic}/captures/{capture}"),
            Some(token),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn a_path_is_made_once_like_mkdir_and_listed_with_its_paths() {
        let (port, [ana, bruno, _]) = demo().await;
        let made = make(port, &ana, "demo", "onboarding/v2/step1").await;
        assert_eq!(status(&made), StatusCode::CREATED, "{made}");
        let leaf: Topic = json(&made);
        assert_eq!(leaf.path, "onboarding/v2/step1");
        let again = make(port, &ana, "demo", "Onboarding/V2/step1").await;
        assert_eq!(
            status(&again),
            StatusCode::OK,
            "names are matched without regard to case"
        );
        assert_eq!(json::<Topic>(&again).id, leaf.id);
        make(port, &ana, "demo", "fix").await;
        let paths: Vec<String> = tree(port, &bruno)
            .await
            .into_iter()
            .map(|t| t.path)
            .collect();
        assert_eq!(
            paths,
            ["fix", "onboarding", "onboarding/v2", "onboarding/v2/step1"]
        );
        for bad in ["a//b", " a", "a/b/", ""] {
            let refused = make(port, &ana, "demo", bad).await;
            assert_eq!(
                status(&refused),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_capture_belongs_to_every_topic_above_where_it_is_filed() {
        let (port, [ana, bruno, _]) = demo().await;
        let step: Topic = json(&make(port, &ana, "demo", "onboarding/v2/step1").await);
        let fix: Topic = json(&make(port, &ana, "demo", "fix").await);
        assert_eq!(
            status(&file(port, &ana, &step.id, A).await),
            StatusCode::NO_CONTENT
        );
        file(port, &ana, &step.id, A).await;
        file(port, &ana, &fix.id, B).await;
        let onboarding = tree(port, &ana)
            .await
            .into_iter()
            .find(|t| t.path == "onboarding")
            .unwrap();
        assert_eq!(ids_under(port, &bruno, &onboarding.id, false).await, [A]);
        assert_eq!(
            ids_under(port, &bruno, &onboarding.id, true).await,
            Vec::<String>::new()
        );
        let v2 = tree(port, &ana)
            .await
            .into_iter()
            .find(|t| t.path == "onboarding/v2")
            .unwrap();
        let moved = call(
            port,
            "PUT",
            &format!("/v1/topics/{}/parent", v2.id),
            Some(&ana),
            Some(
                &serde_json::json!({ "parent_id": fix.id, "generation": v2.generation })
                    .to_string(),
            ),
        )
        .await;
        assert_eq!(status(&moved), StatusCode::OK, "{moved}");
        assert_eq!(json::<Topic>(&moved).path, "fix/v2");
        assert_eq!(ids_under(port, &bruno, &fix.id, false).await, [A, B]);
        assert_eq!(
            ids_under(port, &bruno, &onboarding.id, false).await,
            Vec::<String>::new()
        );
        call(
            port,
            "DELETE",
            &format!("/v1/topics/{}/captures/{B}", fix.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(ids_under(port, &bruno, &fix.id, false).await, [A]);
    }

    #[tokio::test]
    async fn renames_and_moves_check_their_generation_names_and_loops() {
        let (port, [ana, ..]) = demo().await;
        let step: Topic = json(&make(port, &ana, "demo", "onboarding/v2").await);
        make(port, &ana, "demo", "onboarding/v3").await;
        let rename = |name: &str, generation: i64| {
            let (body, path) = (
                serde_json::json!({ "name": name, "generation": generation }).to_string(),
                format!("/v1/topics/{}/name", step.id),
            );
            let token = ana.clone();
            async move { call(port, "PUT", &path, Some(&token), Some(&body)).await }
        };
        let renamed = rename("second", 0).await;
        assert_eq!(status(&renamed), StatusCode::OK, "{renamed}");
        assert_eq!(json::<Topic>(&renamed).path, "onboarding/second");
        assert_eq!(
            status(&rename("third", 0).await),
            StatusCode::CONFLICT,
            "stale generation"
        );
        let clash = rename("V3", 1).await;
        assert_eq!(status(&clash), StatusCode::CONFLICT, "{clash}");
        assert!(body(&clash).contains("already has that name"), "{clash}");
        let onboarding = tree(port, &ana)
            .await
            .into_iter()
            .find(|t| t.path == "onboarding")
            .unwrap();
        let looping = call(
            port,
            "PUT",
            &format!("/v1/topics/{}/parent", onboarding.id),
            Some(&ana),
            Some(
                &serde_json::json!({ "parent_id": step.id, "generation": onboarding.generation })
                    .to_string(),
            ),
        )
        .await;
        assert_eq!(
            status(&looping),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{looping}"
        );
    }

    #[tokio::test]
    async fn a_topic_with_subtopics_goes_only_when_asked_to_take_them_along() {
        let (port, [ana, ..]) = demo().await;
        let leaf: Topic = json(&make(port, &ana, "demo", "fix/123").await);
        file(port, &ana, &leaf.id, A).await;
        let fix = tree(port, &ana)
            .await
            .into_iter()
            .find(|t| t.path == "fix")
            .unwrap();
        let refused = call(
            port,
            "DELETE",
            &format!("/v1/topics/{}", fix.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&refused), StatusCode::CONFLICT, "{refused}");
        let removed = call(
            port,
            "DELETE",
            &format!("/v1/topics/{}?recursive=true", fix.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&removed), StatusCode::NO_CONTENT, "{removed}");
        assert_eq!(tree(port, &ana).await, Vec::<Topic>::new());
        let capture = call(port, "GET", &format!("/v1/captures/{A}"), Some(&ana), None).await;
        assert_eq!(status(&capture), StatusCode::OK, "the capture itself stays");
    }

    #[tokio::test]
    async fn readers_see_topics_and_nobody_files_across_projects() {
        let (port, [ana, bruno, carla]) = demo().await;
        let fix: Topic = json(&make(port, &ana, "demo", "fix").await);
        assert_eq!(
            status(&make(port, &bruno, "demo", "mine").await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&file(port, &bruno, &fix.id, A).await),
            StatusCode::FORBIDDEN
        );
        let theirs: Topic = json(&make(port, &carla, "other", "theirs").await);
        assert_eq!(
            status(&file(port, &carla, &theirs.id, A).await),
            StatusCode::NOT_FOUND,
            "a capture of another project cannot be filed"
        );
        let outsider = call(port, "GET", "/v1/projects/demo/topics", Some(&carla), None).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
    }
}
