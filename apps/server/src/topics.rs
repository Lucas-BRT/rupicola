//! A project's topics: making a path of them, renaming and moving one, removing it, and filing
//! captures under it. A capture filed under a topic belongs to the whole path above it.

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    projects::{Access, access, audit, person},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rupicola_api::{
    CaptureSummary, NewTopic, Role, TOPIC_SEPARATOR, Topic, TopicMove, TopicRename, TopicTree,
    valid_topic_name,
};
use serde::Deserialize;
use std::collections::HashMap;

/// What the database says when a name is already taken among siblings.
const NAME_TAKEN: &str = "UNIQUE constraint failed";
/// What the database says when a move would put a topic under itself.
const TOPIC_CYCLE: &str = "topic cycle";
const NAME_TAKEN_MESSAGE: &str = "another topic there already has that name";
const STALE_MESSAGE: &str = "the topic changed since it was read; read it again";

/// Who may do what: people by role; a capture device may read and file its captures.
async fn allow(
    state: &Shared,
    principal: &Principal,
    project: &str,
    role: Role,
    device: bool,
) -> Result<(), ApiError> {
    match access(state, principal, project).await? {
        Access::Device if device => Ok(()),
        Access::Device => Err(ApiError::Forbidden),
        person @ Access::Person(_) => person.require(role),
    }
}

/// The project a topic belongs to, after checking the caller's access.
async fn topic_project(
    state: &Shared,
    principal: &Principal,
    topic: &str,
    role: Role,
    device: bool,
) -> Result<String, ApiError> {
    let project: String = sqlx::query_scalar("SELECT project_id FROM topics WHERE id = ?1")
        .bind(topic)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    allow(state, principal, &project, role, device).await?;
    Ok(project)
}

/// One topic with its path and the captures filed directly under it.
async fn one(state: &Shared, topic: &str) -> Result<Topic, ApiError> {
    let (id, parent_id, name, position, generation): (String, Option<String>, String, i64, i64) =
        sqlx::query_as(
            "SELECT id, parent_id, name, position, generation FROM topics WHERE id = ?1",
        )
        .bind(topic)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let names: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE up(id, parent_id, name, depth) AS (
           SELECT id, parent_id, name, 0 FROM topics WHERE id = ?1
           UNION ALL
           SELECT t.id, t.parent_id, t.name, up.depth + 1 FROM topics t JOIN up ON t.id = up.parent_id)
         SELECT name FROM up ORDER BY depth DESC",
    )
    .bind(topic)
    .fetch_all(&state.db)
    .await?;
    let captures: i64 =
        sqlx::query_scalar("SELECT count(*) FROM capture_topics WHERE topic_id = ?1")
            .bind(topic)
            .fetch_one(&state.db)
            .await?;
    Ok(Topic {
        id,
        parent_id,
        name,
        path: names.join(&TOPIC_SEPARATOR.to_string()),
        position,
        generation,
        captures: u64::try_from(captures).unwrap_or_default(),
    })
}

/// A database error read as the refusal it stands for.
fn refusal(error: sqlx::Error) -> ApiError {
    let text = error.to_string();
    if text.contains(NAME_TAKEN) {
        ApiError::Conflict(NAME_TAKEN_MESSAGE)
    } else if text.contains(TOPIC_CYCLE) || text.contains("CHECK constraint failed") {
        ApiError::invalid("a topic cannot go under itself or one of its subtopics")
    } else {
        error.into()
    }
}

/// `GET /v1/projects/{id}/topics`: every topic of the project, ordered by path.
pub async fn tree(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<TopicTree>, ApiError> {
    allow(&state, &principal, &project, Role::Reader, true).await?;
    let rows: Vec<(String, Option<String>, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT t.id, t.parent_id, t.name, t.position, t.generation,
           (SELECT count(*) FROM capture_topics c WHERE c.topic_id = t.id)
         FROM topics t WHERE t.project_id = ?1",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    let parents: HashMap<String, (Option<String>, String)> = rows
        .iter()
        .map(|(id, parent, name, ..)| (id.clone(), (parent.clone(), name.clone())))
        .collect();
    let path_of = |start: &str| {
        let mut names = Vec::new();
        let mut id = start.to_owned();
        while let Some((parent, name)) = parents.get(&id) {
            names.push(name.clone());
            match parent {
                Some(above) => id.clone_from(above),
                None => break,
            }
        }
        names.reverse();
        names.join(&TOPIC_SEPARATOR.to_string())
    };
    let mut topics: Vec<Topic> = rows
        .iter()
        .map(
            |(id, parent_id, name, position, generation, captures)| Topic {
                id: id.clone(),
                parent_id: parent_id.clone(),
                name: name.clone(),
                path: path_of(id),
                position: *position,
                generation: *generation,
                captures: u64::try_from(*captures).unwrap_or_default(),
            },
        )
        .collect();
    topics.sort_by_cached_key(|topic| topic.path.to_lowercase());
    Ok(Json(TopicTree { topics }))
}

/// `POST /v1/projects/{id}/topics`: makes every topic of the path that is missing, like
/// `mkdir -p`, and returns the last one. 201 when something was made, 200 when it all existed.
/// Names are matched without regard to case.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewTopic>,
) -> Result<(StatusCode, Json<Topic>), ApiError> {
    allow(&state, &principal, &project, Role::Reviewer, true).await?;
    let names: Vec<&str> = form.path.split(TOPIC_SEPARATOR).collect();
    if let Some(bad) = names.iter().find(|name| !valid_topic_name(name)) {
        return Err(ApiError::invalid(format!(
            "`{bad}` cannot name a topic: use 1 to 64 characters, no `/`, no spaces at the ends"
        )));
    }
    let mut parent: Option<String> = None;
    let mut made = false;
    for name in names {
        let find = || async {
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM topics WHERE project_id = ?1 AND parent_id IS ?2
                   AND lower(name) = lower(?3)",
            )
            .bind(&project)
            .bind(&parent)
            .bind(name)
            .fetch_optional(&state.db)
            .await
        };
        let id = if let Some(id) = find().await? {
            id
        } else {
            let id = uuid::Uuid::now_v7().to_string();
            let inserted = sqlx::query(
                "INSERT INTO topics (id, project_id, parent_id, name, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(&id)
            .bind(&project)
            .bind(&parent)
            .bind(name)
            .bind(now_ms())
            .execute(&state.db)
            .await;
            match inserted {
                Ok(_) => {
                    made = true;
                    id
                }
                // Someone made the same topic at the same moment; use theirs.
                Err(error) if error.to_string().contains(NAME_TAKEN) => find()
                    .await?
                    .ok_or(ApiError::Conflict(NAME_TAKEN_MESSAGE))?,
                Err(error) => return Err(error.into()),
            }
        };
        parent = Some(id);
    }
    let leaf = parent.ok_or_else(|| ApiError::invalid("name at least one topic"))?;
    let status = if made {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(one(&state, &leaf).await?)))
}

/// `PUT /v1/topics/{id}/name`: renames a topic; what is filed under it stays.
pub async fn rename(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Json(form): Json<TopicRename>,
) -> Result<Json<Topic>, ApiError> {
    let project = topic_project(&state, &principal, &topic, Role::Reviewer, false).await?;
    let actor = person(&principal)?.to_owned();
    if !valid_topic_name(&form.name) {
        return Err(ApiError::invalid(
            "a topic name has 1 to 64 characters, no `/` and no spaces at the ends",
        ));
    }
    let changed = sqlx::query(
        "UPDATE topics SET name = ?1, generation = generation + 1 WHERE id = ?2 AND generation = ?3",
    )
    .bind(&form.name)
    .bind(&topic)
    .bind(form.generation)
    .execute(&state.db)
    .await
    .map_err(refusal)?
    .rows_affected();
    if changed == 0 {
        return Err(ApiError::Conflict(STALE_MESSAGE));
    }
    audit(
        &state.db,
        Some(&project),
        &actor,
        &format!("Renamed a topic to {}", form.name),
    )
    .await?;
    Ok(Json(one(&state, &topic).await?))
}

/// `PUT /v1/topics/{id}/parent`: moves a topic, with everything under it, to another parent
/// of the same project, or to the root.
pub async fn move_to(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Json(form): Json<TopicMove>,
) -> Result<Json<Topic>, ApiError> {
    let project = topic_project(&state, &principal, &topic, Role::Reviewer, false).await?;
    let actor = person(&principal)?.to_owned();
    if let Some(parent) = &form.parent_id {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT project_id FROM topics WHERE id = ?1")
                .bind(parent)
                .fetch_optional(&state.db)
                .await?;
        if owner.as_deref() != Some(project.as_str()) {
            return Err(ApiError::invalid(
                "the new parent is not a topic of this project",
            ));
        }
    }
    let changed = sqlx::query(
        "UPDATE topics SET parent_id = ?1, generation = generation + 1
         WHERE id = ?2 AND generation = ?3",
    )
    .bind(&form.parent_id)
    .bind(&topic)
    .bind(form.generation)
    .execute(&state.db)
    .await
    .map_err(refusal)?
    .rows_affected();
    if changed == 0 {
        return Err(ApiError::Conflict(STALE_MESSAGE));
    }
    audit(&state.db, Some(&project), &actor, "Moved a topic").await?;
    Ok(Json(one(&state, &topic).await?))
}

#[derive(Deserialize)]
pub struct Removal {
    #[serde(default)]
    recursive: bool,
}

/// `DELETE /v1/topics/{id}`: removes a topic and unfiles its captures, which stay. A topic with
/// subtopics goes only with `?recursive=true`, and takes them along.
pub async fn remove(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Query(removal): Query<Removal>,
) -> Result<StatusCode, ApiError> {
    let project = topic_project(&state, &principal, &topic, Role::Reviewer, false).await?;
    let actor = person(&principal)?.to_owned();
    let children: i64 = sqlx::query_scalar("SELECT count(*) FROM topics WHERE parent_id = ?1")
        .bind(&topic)
        .fetch_one(&state.db)
        .await?;
    if children > 0 && !removal.recursive {
        return Err(ApiError::Conflict(
            "the topic has subtopics; remove them first or ask for a recursive removal",
        ));
    }
    sqlx::query(
        "WITH RECURSIVE tree(id) AS (
           SELECT ?1 UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
         DELETE FROM topics WHERE id IN (SELECT id FROM tree)",
    )
    .bind(&topic)
    .execute(&state.db)
    .await?;
    audit(&state.db, Some(&project), &actor, "Removed a topic").await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Listing {
    #[serde(default)]
    direct: bool,
}

/// `GET /v1/topics/{id}/captures`: the captures filed under the topic or any topic below it,
/// newest first; `?direct=true` leaves out its subtopics.
pub async fn captures(
    State(state): State<Shared>,
    principal: Principal,
    Path(topic): Path<String>,
    Query(listing): Query<Listing>,
) -> Result<Json<Vec<CaptureSummary>>, ApiError> {
    topic_project(&state, &principal, &topic, Role::Reader, true).await?;
    let ids: Vec<String> = if listing.direct {
        sqlx::query_scalar("SELECT capture_id FROM capture_topics WHERE topic_id = ?1")
            .bind(&topic)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_scalar(
            "WITH RECURSIVE tree(id) AS (
               SELECT ?1 UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
             SELECT DISTINCT c.capture_id FROM capture_topics c JOIN tree ON c.topic_id = tree.id",
        )
        .bind(&topic)
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(crate::captures::summaries(&state, &ids).await?))
}

/// `PUT /v1/topics/{id}/captures/{capture}`: files a capture of the same project under the
/// topic. Filing it again changes nothing.
pub async fn file(
    State(state): State<Shared>,
    principal: Principal,
    Path((topic, capture)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let project = topic_project(&state, &principal, &topic, Role::Reviewer, true).await?;
    let owner: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1 AND deleted_at IS NULL")
            .bind(&capture)
            .fetch_optional(&state.db)
            .await?;
    if owner.as_deref() != Some(project.as_str()) {
        return Err(ApiError::NotFound);
    }
    sqlx::query(
        "INSERT OR IGNORE INTO capture_topics (project_id, topic_id, capture_id, added_by, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&project)
    .bind(&topic)
    .bind(&capture)
    .bind(principal.handle.as_deref())
    .bind(now_ms())
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/topics/{id}/captures/{capture}`: takes a capture out of the topic.
pub async fn unfile(
    State(state): State<Shared>,
    principal: Principal,
    Path((topic, capture)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    topic_project(&state, &principal, &topic, Role::Reviewer, true).await?;
    sqlx::query("DELETE FROM capture_topics WHERE topic_id = ?1 AND capture_id = ?2")
        .bind(&topic)
        .bind(&capture)
        .execute(&state.db)
        .await?;
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
