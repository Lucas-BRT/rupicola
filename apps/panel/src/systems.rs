//! The design systems of a project as the design tab shows them.

use crate::PageResult;
use crate::backend;
use rupicola_api::{SystemList, SystemVersion};
use topcoat::context::Cx;

/// The versions of a project's design system and the one being looked at.
pub struct Studio {
    pub list: SystemList,
    /// The version asked for, else the adopted one, else the newest.
    pub selected: Option<SystemVersion>,
}

/// The studio of `project`, with `selected` open.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn load(cx: &Cx, project: &str, selected: Option<&str>) -> PageResult<Studio> {
    let list = backend::design_systems(cx, project)
        .await
        .as_ref()
        .map_err(|e| *e)?
        .clone();
    let open = selected
        .or(list.active.as_deref())
        .or_else(|| list.items.first().map(|item| item.id.as_str()));
    let selected = match open {
        Some(id) => Some(
            backend::design_system(cx, project, id)
                .await
                .as_ref()
                .map_err(|e| *e)?
                .clone(),
        ),
        None => None,
    };
    Ok(Studio { list, selected })
}
