//! `GET /projects/{project}/findings`: the problems tab.

use super::tab_metadata;
use crate::views::project::Tab;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::View,
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Problems).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    super::problems::show(cx, parse_query_params(cx)?).await
}
