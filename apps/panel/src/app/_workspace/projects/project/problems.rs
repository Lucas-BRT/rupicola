//! `GET /projects/{project}/problems`

mod analyze;
mod create;
mod problem;

use super::{current, tab_metadata};
use crate::app::fail;
use crate::views::project::Tab;
use crate::{project::Filter, records, views::project::problems_tab};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Problems).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    show(cx, parse_query_params(cx)?).await
}

/// The problems tab, which `findings` also opens.
pub(super) async fn show(cx: &Cx, filter: Filter) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let assets = project.assets();
    let fixes = records::fixes(cx, &project.id).await.map_err(fail)?;
    let problems = records::problems(cx, &project.id, &assets, &fixes)
        .await
        .map_err(fail)?;
    let design = records::design(cx, &project.id).await.map_err(fail)?;
    Ok(view! {
        problems_tab(role: project.role, base: &project.base, filter: filter, problems: problems, assets: assets, design_entries: design)
    })
}
