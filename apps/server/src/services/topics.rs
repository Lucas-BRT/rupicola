//! A project's topics: making a path of them, renaming and moving one, removing it, and filing
//! captures under it. A capture filed under a topic belongs to the whole path above it.

use crate::{
    domain::{
        ApiError, Principal,
        secret::now_ms,
        topic::{self, NAME_TAKEN_MESSAGE, check_name, names},
    },
    repositories::{audit, captures, lineage, topics},
    services::access::{require, require_publisher},
    state::AppState,
};
use rupicola_api::{CaptureSummary, NewTopic, Role, Topic, TopicMove, TopicTree};

/// Who may do what: people by role; a capture device may read and file its captures.
async fn allow(
    state: &AppState,
    principal: &Principal,
    project: &str,
    role: Role,
    device: bool,
) -> Result<(), ApiError> {
    if device {
        require_publisher(state, principal, project, role).await
    } else {
        require(state, principal, project, role).await
    }
}

/// The project a topic belongs to, after checking the caller's access.
async fn topic_project(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    role: Role,
    device: bool,
) -> Result<String, ApiError> {
    let project = topics::project_of(&state.db, topic)
        .await?
        .ok_or(ApiError::NotFound)?;
    allow(state, principal, &project, role, device).await?;
    Ok(project)
}

async fn one(state: &AppState, topic: &str) -> Result<Topic, ApiError> {
    topics::one(&state.db, topic)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Every topic of the project, ordered by path.
pub async fn tree(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<TopicTree, ApiError> {
    allow(state, principal, project, Role::Reader, true).await?;
    Ok(TopicTree {
        topics: topic::tree(topics::of_project(&state.db, project).await?),
    })
}

/// Makes every topic of the path that is missing, like `mkdir -p`, and returns the last one
/// and whether anything was made. Names are matched without regard to case.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewTopic,
) -> Result<(Topic, bool), ApiError> {
    allow(state, principal, project, Role::Reviewer, true).await?;
    let path_names = names(&form.path)?;
    let mut tx = state.db.begin().await?;
    let mut parent: Option<String> = None;
    let mut made = false;
    for name in path_names {
        let id = if let Some(id) = topics::child(&mut *tx, project, parent.as_deref(), name).await?
        {
            id
        } else {
            let id = uuid::Uuid::now_v7().to_string();
            if topics::insert(&mut *tx, &id, project, parent.as_deref(), name, now_ms()).await? {
                made = true;
                id
            } else {
                // Someone made the same topic at the same moment; use theirs.
                topics::child(&mut *tx, project, parent.as_deref(), name)
                    .await?
                    .ok_or(ApiError::Conflict(NAME_TAKEN_MESSAGE))?
            }
        };
        parent = Some(id);
    }
    let leaf = parent.ok_or_else(|| ApiError::invalid("name at least one topic"))?;
    if made {
        audit::record(
            &mut *tx,
            Some(project),
            principal.actor(),
            &format!("Created topic {}", form.path.trim()),
        )
        .await?;
    }
    tx.commit().await?;
    Ok((one(state, &leaf).await?, made))
}

/// Renames a topic; what is filed under it stays.
pub async fn rename(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    name: &str,
    generation: i64,
) -> Result<Topic, ApiError> {
    let project = topic_project(state, principal, topic, Role::Reviewer, false).await?;
    let actor = principal.person()?;
    check_name(name)?;
    let mut tx = state.db.begin().await?;
    topics::rename(&mut *tx, topic, name, generation)
        .await?
        .done()?;
    audit::record(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Renamed a topic to {name}"),
    )
    .await?;
    tx.commit().await?;
    one(state, topic).await
}

/// Moves a topic, with everything under it, to another parent of the same project, or to
/// the root.
pub async fn move_to(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    form: &TopicMove,
) -> Result<Topic, ApiError> {
    let project = topic_project(state, principal, topic, Role::Reviewer, false).await?;
    let actor = principal.person()?;
    let mut tx = state.db.begin().await?;
    if let Some(parent) = &form.parent_id
        && topics::project_of(&mut *tx, parent).await?.as_deref() != Some(project.as_str())
    {
        return Err(ApiError::invalid(
            "the new parent is not a topic of this project",
        ));
    }
    topics::move_to(&mut *tx, topic, form.parent_id.as_deref(), form.generation)
        .await?
        .done()?;
    audit::record(&mut *tx, Some(&project), actor, "Moved a topic").await?;
    tx.commit().await?;
    one(state, topic).await
}

/// Removes a topic and unfiles its captures, which stay. A topic with subtopics goes only
/// when `recursive`, and takes them along.
pub async fn remove(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    recursive: bool,
) -> Result<(), ApiError> {
    let project = topic_project(state, principal, topic, Role::Reviewer, false).await?;
    let actor = principal.person()?;
    let mut tx = state.db.begin().await?;
    if topics::children(&mut *tx, topic).await? > 0 && !recursive {
        return Err(ApiError::Conflict(
            "the topic has subtopics; remove them first or ask for a recursive removal",
        ));
    }
    topics::delete_tree(&mut *tx, topic).await?;
    audit::record(&mut *tx, Some(&project), actor, "Removed a topic").await?;
    tx.commit().await?;
    Ok(())
}

/// The captures filed under the topic or any topic below it, newest first; `direct` leaves
/// out its subtopics.
pub async fn captures(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    direct: bool,
) -> Result<Vec<CaptureSummary>, ApiError> {
    topic_project(state, principal, topic, Role::Reader, true).await?;
    let ids = if direct {
        topics::direct_captures(&state.db, topic).await?
    } else {
        topics::all_captures(&state.db, topic).await?
    };
    captures::summaries(&state.db, &ids).await
}

/// Files a capture of the same project under the topic. Filing it again changes nothing.
pub async fn file(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    capture: &str,
) -> Result<(), ApiError> {
    let project = topic_project(state, principal, topic, Role::Reviewer, true).await?;
    if lineage::live_project(&state.db, capture).await?.as_deref() != Some(project.as_str()) {
        return Err(ApiError::NotFound);
    }
    topics::file(
        &state.db,
        &project,
        topic,
        capture,
        principal.handle.as_deref(),
        now_ms(),
    )
    .await?;
    Ok(())
}

/// Takes a capture out of the topic.
pub async fn unfile(
    state: &AppState,
    principal: &Principal,
    topic: &str,
    capture: &str,
) -> Result<(), ApiError> {
    topic_project(state, principal, topic, Role::Reviewer, true).await?;
    topics::unfile(&state.db, topic, capture).await?;
    Ok(())
}
