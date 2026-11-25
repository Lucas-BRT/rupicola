//! `GET /projects/{project}/captures`

use super::{current, tab_metadata};
use crate::app::fail;
use crate::views::project::Tab;
use crate::{project::Filter, records, views::project::captures_tab};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Captures).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    show(cx, parse_query_params(cx)?).await
}

/// The captures tab, which `runs`, `images` and `videos` also open with another view.
pub(super) async fn show(cx: &Cx, filter: Filter) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let assets = project.assets();
    let problems = records::problems(cx, &project.id, &assets, &[])
        .await
        .map_err(fail)?;
    Ok(view! {
        captures_tab(role: project.role, base: &project.base, filter: filter, runs: project.related(), assets: assets, problems: problems)
    })
}
