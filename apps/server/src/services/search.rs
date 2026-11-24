//! Capture search use case: parses filters, pages by cursor and optionally loads full captures.

use crate::{
    domain::{
        Access, ApiError, Principal,
        search::{CURSOR_SEPARATOR, Search},
    },
    repositories::{captures, search},
    services::access::access,
    state::AppState,
};
use rupicola_api::{CapturePage, Role};

/// Searches a project's captures, newest first, returning one page and an optional next cursor.
pub async fn find(
    state: &AppState,
    principal: &Principal,
    project: &str,
    query: Option<&str>,
) -> Result<CapturePage, ApiError> {
    match access(state, principal, project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reader)?,
    }
    let parsed = Search::parse(query)?;
    let mut rows = search::capture_ids(&state.db, project, &parsed).await?;
    let page = usize::try_from(parsed.limit).unwrap_or(usize::MAX);
    let next = (rows.len() > page).then(|| {
        rows.truncate(page);
        rows.last()
            .map(|(id, at)| format!("{at}{CURSOR_SEPARATOR}{id}"))
            .unwrap_or_default()
    });
    let ids: Vec<String> = rows.into_iter().map(|(id, _)| id).collect();
    let items = captures::summaries(&state.db, &ids).await?;
    let mut details = Vec::new();
    if parsed.details {
        for id in &ids {
            details.push(captures::load(&state.db, id).await?);
        }
    }
    Ok(CapturePage {
        items,
        details,
        next,
    })
}
