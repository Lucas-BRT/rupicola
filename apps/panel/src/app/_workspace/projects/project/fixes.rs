//! `GET /projects/{project}/fixes`

mod create;
mod fix;

use super::{current, tab_metadata};
use crate::app::fail;
use crate::views::project::Tab;
use crate::{project::Filter, records, views::project::fixes_tab};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Fixes).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let assets = project.assets();
    let fixes = records::fixes(cx, &project.id).await.map_err(fail)?;
    let problems = records::problems(cx, &project.id, &assets, &fixes)
        .await
        .map_err(fail)?;
    Ok(view! {
        fixes_tab(role: project.role, base: &project.base, filter: filter, fixes: fixes, problems: problems, assets: assets)
    })
}
