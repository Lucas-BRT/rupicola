//! Selections: frozen lists of anchors into a project's captures, which problems and fixes
//! cite as proof. A selection is never changed; a new proof is a new selection.

use crate::{
    domain::{Access, ApiError, Principal, secret::now_ms, selection},
    repositories::selections,
    services::access::access,
    state::AppState,
};
use rupicola_api::{NewSelection, Role, Selection};
use sqlx::SqliteConnection;

/// Keeps `form` as a selection of `project` inside `tx`; returns its id. Every anchor must be a
/// live capture of the project, and a file one of that capture's.
///
/// # Errors
///
/// [`ApiError::Invalid`] naming the first anchor that is not.
pub async fn keep(
    tx: &mut SqliteConnection,
    project: &str,
    author: Option<&str>,
    form: &NewSelection,
) -> Result<String, ApiError> {
    let label = selection::validate(form)?;
    let id = uuid::Uuid::now_v7().to_string();
    selections::insert(tx, &id, project, label, author, now_ms()).await?;
    for (position, item) in form.items.iter().enumerate() {
        let artifact = selections::resolve_item(tx, project, item).await?;
        let fragment = item
            .fragment
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(anyhow::Error::from)?;
        selections::insert_item(
            tx,
            project,
            &id,
            i64::try_from(position).map_err(anyhow::Error::from)?,
            &item.capture,
            artifact.as_deref(),
            item.mark.as_str(),
            fragment.as_deref(),
        )
        .await?;
    }
    Ok(id)
}

/// Keeps a new selection in `project` and returns it.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewSelection,
) -> Result<Selection, ApiError> {
    match access(state, principal, project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    let mut tx = state.db.begin().await?;
    let id = keep(&mut tx, project, principal.handle.as_deref(), form).await?;
    tx.commit().await?;
    let mut kept = selections::load(&state.db, std::slice::from_ref(&id)).await?;
    Ok(kept.remove(0))
}

/// Loads one selection after checking the caller may read its project.
pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Selection, ApiError> {
    let project = selections::project_of(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    match access(state, principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reader)?,
    }
    let mut kept = selections::load(&state.db, std::slice::from_ref(&id.to_owned())).await?;
    Ok(kept.remove(0))
}
