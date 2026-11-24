//! `GET /architecture`

use super::cfg;
use crate::views::architecture_page;
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::page,
    view::{View, view},
};

#[page]
async fn architecture(cx: &Cx) -> TopcoatResult<impl View> {
    let config = cfg(cx);
    Ok(view! { architecture_page(panel_url: &config.panel_url) })
}
