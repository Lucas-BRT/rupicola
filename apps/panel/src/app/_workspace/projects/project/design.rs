//! `GET /projects/{project}/design`

mod edit;
mod preview;
mod proof;
mod record;
mod version;

use super::{current, tab_metadata};
use crate::app::fail;
use crate::views::project::Tab;
use crate::{project::Filter, records, systems, views::project::design::design_tab};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Design).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let records = records::design(cx, &project.id).await.map_err(fail)?;
    let tab = match filter.view.as_deref() {
        Some("system") => "system",
        Some("screens") => "screens",
        _ => "records",
    };
    let studio = systems::load(cx, &project.id, filter.version.as_deref())
        .await
        .map_err(fail)?;
    Ok(view! {
        design_tab(base: &project.base, role: project.role, tab: tab, records: records, all_assets: project.assets(), studio: studio)
    })
}
