//! Reading and editing the parents of a capture. The record keeps the parents it was published
//! with; the lineage that holds now lives in `capture_parents`, and every edit leaves an event.

use crate::{
    domain::{
        ApiError, Principal,
        lineage::{check_edit, check_known, cycle, depth, diff},
        secret::now_ms,
    },
    repositories::{
        audit,
        lineage::{self, Event},
        records,
    },
    services::access::{require, require_publisher, require_reader},
    state::AppState,
};
use rupicola_api::{CaptureDiff, Lineage, LineageGraph, ParentsEdit, Role, record::ParentRef};

/// The project of a live capture, after checking the caller may act there as `role`. A device
/// may only read.
async fn project_of(
    state: &AppState,
    principal: &Principal,
    capture: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project = lineage::live_project(&state.db, capture)
        .await?
        .ok_or(ApiError::NotFound)?;
    if role == Role::Reader {
        require_reader(state, principal, &project).await?;
    } else {
        require(state, principal, &project, role).await?;
    }
    Ok(project)
}

/// The captures this one descends from and those that descend from it, `depth` steps either
/// way, with the links between them.
pub async fn graph(
    state: &AppState,
    principal: &Principal,
    capture: &str,
    asked: Option<u32>,
) -> Result<LineageGraph, ApiError> {
    project_of(state, principal, capture, Role::Reader).await?;
    let ids = lineage::reach(&state.db, capture, depth(asked)?).await?;
    Ok(LineageGraph {
        root: capture.to_owned(),
        nodes: lineage::nodes(&state.db, &ids).await?,
        links: lineage::links(&state.db, &ids).await?,
    })
}

/// What changed from capture `before` to capture `after` of the same project. Removed captures
/// can be compared.
pub async fn compare(
    state: &AppState,
    principal: &Principal,
    before: &str,
    after: &str,
) -> Result<CaptureDiff, ApiError> {
    let owners = lineage::owners(&state.db, before, after).await?;
    let found = |id: &str| {
        owners
            .iter()
            .find(|owner| owner.id == id)
            .map(|owner| &owner.project_id)
    };
    let (Some(project), Some(other)) = (found(before), found(after)) else {
        return Err(ApiError::NotFound);
    };
    if project != other {
        return Err(ApiError::NotFound);
    }
    require_publisher(state, principal, project, Role::Reader).await?;
    Ok(diff(
        (
            before.to_owned(),
            lineage::contents(&state.db, before).await?,
        ),
        (after.to_owned(), lineage::contents(&state.db, after).await?),
    ))
}

/// The lineage of `capture` as it stands.
async fn load(state: &AppState, capture: &str) -> Result<Lineage, ApiError> {
    Ok(Lineage {
        parents: lineage::parents(&state.db, capture).await?,
        generation: lineage::generation(&state.db, capture).await?,
        history: lineage::history(&state.db, capture).await?,
    })
}

/// The parents of a capture, where each came from, and the history of its edits.
pub async fn get(
    state: &AppState,
    principal: &Principal,
    capture: &str,
) -> Result<Lineage, ApiError> {
    project_of(state, principal, capture, Role::Reader).await?;
    load(state, capture).await
}

/// Replaces the parents of a capture, if the lineage is still the generation the edit read.
/// Every parent must be a live capture of the same project, and no parent may make the
/// lineage a loop.
pub async fn edit(
    state: &AppState,
    principal: &Principal,
    capture: &str,
    form: &ParentsEdit,
) -> Result<Lineage, ApiError> {
    let project = project_of(state, principal, capture, Role::Reviewer).await?;
    let actor = principal.person()?;
    let reason = check_edit(capture, form)?;
    let ids: Vec<&str> = form.parents.iter().map(|p| p.id.as_str()).collect();
    let mut tx = state.db.begin().await?;
    check_known(&ids, &records::live(&mut *tx, &project, &ids).await?)?;
    let before: Vec<ParentRef> = lineage::parents(&mut *tx, capture)
        .await?
        .into_iter()
        .map(|edge| ParentRef {
            id: edge.id,
            kind: edge.kind,
        })
        .collect();
    if !lineage::advance(&mut *tx, capture, form.generation).await? {
        return Err(ApiError::Conflict(
            "the lineage changed since it was read; read it again",
        ));
    }
    if let Some(parent) = lineage::replace(&mut tx, &project, capture, &form.parents).await? {
        return Err(cycle(&parent));
    }
    lineage::insert_event(
        &mut *tx,
        &Event {
            capture,
            generation: form.generation + 1,
            actor,
            reason,
            before: &before,
            after: &form.parents,
            at: now_ms(),
        },
    )
    .await?;
    audit::record(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Changed the parents of capture {capture}"),
    )
    .await?;
    tx.commit().await?;
    load(state, capture).await
}
