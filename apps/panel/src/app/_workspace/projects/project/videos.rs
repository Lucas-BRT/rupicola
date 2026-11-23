//! `GET /projects/{project}/videos`: the captures tab with the videos view.

use super::tab_metadata;
use crate::project::Filter;
use crate::views::project::Tab;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::View,
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Captures).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let mut filter: Filter = parse_query_params(cx)?;
    filter.view.get_or_insert_with(|| "videos".into());
    super::captures::show(cx, filter).await
}
