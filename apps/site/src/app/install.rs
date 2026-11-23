//! `GET /install`

use super::cfg;
use crate::views::install_page;
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::page,
    view::{View, view},
};

#[page]
async fn install(cx: &Cx) -> TopcoatResult<impl View> {
    let config = cfg(cx);
    Ok(view! { install_page(panel_url: &config.panel_url, public_url: &config.public_url) })
}
