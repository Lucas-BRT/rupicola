//! Capture search use case: parses filters, pages by cursor and optionally loads full captures.

use crate::{
    domain::{
        ApiError, Principal,
        search::{CURSOR_SEPARATOR, Search},
    },
    repositories::{captures, search},
    services::access::require_reader,
    state::AppState,
};
use rupicola_api::CapturePage;

/// Searches a project's captures, newest first, returning one page and an optional next cursor.
pub async fn find(
    state: &AppState,
    principal: &Principal,
    project: &str,
    parsed: &Search,
) -> Result<CapturePage, ApiError> {
    require_reader(state, principal, project).await?;
    let mut rows = search::capture_ids(&state.db, project, parsed).await?;
    let page = usize::try_from(parsed.limit).unwrap_or(usize::MAX);
    let next = (rows.len() > page).then(|| {
        rows.truncate(page);
        rows.last()
            .map(|(id, at)| format!("{at}{CURSOR_SEPARATOR}{id}"))
            .unwrap_or_default()
    });
    let ids: Vec<String> = rows.into_iter().map(|(id, _)| id).collect();
    let items = captures::summaries(&state.db, &ids).await?;
    let details = if parsed.details {
        captures::load_many(&state.db, &ids).await?
    } else {
        Vec::new()
    };
    Ok(CapturePage {
        items,
        details,
        next,
    })
}
