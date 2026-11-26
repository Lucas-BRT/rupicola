//! `GET /projects/{project}/design`

mod edit;
mod record;
mod version;

use super::{current, tab_metadata};
use crate::app::fail;
use crate::components::skeleton::list_tab_skeleton;
use crate::views::project::Tab;
use crate::{project::Filter, records, systems, views::project::design::design_tab};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, component, error_boundary, suspense, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Design).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    Ok(view! {
        suspense(
            fallback: view! { list_tab_skeleton() },
            error_boundary(
                fallback: |_| Ok(view! {
                    <p class="notice" role="alert">"Design records could not be loaded. Try again in a moment."</p>
                }),
                loaded_design(filter: filter)
            )
        )
    })
}

#[component]
async fn loaded_design(cx: &Cx, filter: Filter) -> Result<impl View> {
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
