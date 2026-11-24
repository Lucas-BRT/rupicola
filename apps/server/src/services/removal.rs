//! Capture removal use cases: listing citations, tombstoning captures and managing
//! project deletion policies.

use crate::{
    domain::{
        Access, ApiError, Principal,
        removal::{check_reason, confirmation_token, is_allowed},
        secret::now_ms,
    },
    repositories::{audit, removal},
    services::access::{access, require_reader},
    state::AppState,
};
use rupicola_api::{CaptureRemoval, Citations, PolicyView, Role};
use std::sync::atomic::Ordering;

/// The project of a live capture, after checking the caller may at least read it.
async fn live_capture(
    state: &AppState,
    principal: &Principal,
    capture: &str,
) -> Result<(String, Access), ApiError> {
    let project = removal::live_capture_project(&state.db, capture)
        .await?
        .ok_or(ApiError::NotFound)?;
    let seen = access(state, principal, &project).await?;
    if let Access::Person(_) = seen {
        seen.require(Role::Reader)?;
    }
    Ok((project, seen))
}

/// Lists what cites `capture`, its direct children and whether removing it is allowed.
pub async fn citations(
    state: &AppState,
    principal: &Principal,
    capture: &str,
) -> Result<Citations, ApiError> {
    let (project, seen) = live_capture(state, principal, capture).await?;
    let citations = removal::cited_by(&state.db, capture).await?;
    let children = removal::children(&state.db, capture).await?;
    let policy = removal::project_policy(&state.db, &project).await?;
    let server_allows = state.cited_deletion.load(Ordering::Relaxed);
    Ok(Citations {
        allowed: is_allowed(server_allows, seen, &citations, policy),
        confirm: (!citations.is_empty()).then(|| confirmation_token(capture, &citations)),
        citations,
        children,
        policy,
    })
}

/// Removes a capture, leaving a tombstone and reopening problems left without live proof.
pub async fn remove(
    state: &AppState,
    principal: &Principal,
    capture: &str,
    form: &CaptureRemoval,
) -> Result<(), ApiError> {
    let (project, seen) = live_capture(state, principal, capture).await?;
    let actor = principal.person()?.to_owned();
    let reason = check_reason(&form.reason)?;
    let mut tx = state.db.begin().await?;
    let citations = removal::cited_by(&mut *tx, capture).await?;
    let policy = removal::project_policy(&mut *tx, &project).await?;
    let server_allows = state.cited_deletion.load(Ordering::Relaxed);
    if !is_allowed(server_allows, seen, &citations, policy) {
        return Err(ApiError::Forbidden);
    }
    if !citations.is_empty()
        && form.confirm.as_deref() != Some(confirmation_token(capture, &citations).as_str())
    {
        return Err(ApiError::Conflict(
            "confirm the citations first: read them again and send back their confirmation",
        ));
    }
    let now = now_ms();
    removal::tombstone(&mut tx, capture, &actor, reason, now).await?;
    removal::reopen_uncited_problems(&mut tx, capture, now).await?;
    let detail =
        serde_json::json!({ "capture": capture, "reason": reason, "citations": citations });
    audit::record_with_detail(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Removed capture {capture}"),
        Some(&detail.to_string()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Reads a project's cited-capture deletion policy together with the server's ceiling.
pub async fn policy(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<PolicyView, ApiError> {
    require_reader(state, principal, project).await?;
    Ok(PolicyView {
        policy: removal::project_policy(&state.db, project).await?,
        server_allows: state.cited_deletion.load(Ordering::Relaxed),
    })
}

/// Sets a project's cited-capture deletion policy (owners only).
pub async fn set_policy(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &PolicyView,
) -> Result<PolicyView, ApiError> {
    access(state, principal, project)
        .await?
        .require(Role::Owner)?;
    let actor = principal.person()?.to_owned();
    let mut tx = state.db.begin().await?;
    removal::set_project_policy(&mut tx, project, form.policy, &actor, now_ms()).await?;
    audit::record(
        &mut *tx,
        Some(project),
        &actor,
        &format!("Set the removal of cited captures to {}", form.policy),
    )
    .await?;
    tx.commit().await?;
    Ok(PolicyView {
        policy: form.policy,
        server_allows: state.cited_deletion.load(Ordering::Relaxed),
    })
}
