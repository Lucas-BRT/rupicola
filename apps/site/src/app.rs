//! The site's routes. Each module below this one is a path segment (`install` serves
//! `/install`); a handler only reads the request and hands data to a view.

mod architecture;
mod assets;
mod docs;
mod healthz;
mod install;

use crate::views::home_page;
use std::sync::Arc;
use topcoat::{
    Result as TopcoatResult,
    context::{Cx, context},
    router::{Router, RouterBuilderDiscoverExt, content::Html, module_router, page},
    view::{View, ViewExt, view},
};

/// Where the site and the review panel live, for links and the install script.
#[derive(Clone)]
pub struct SiteConfig {
    pub public_url: String,
    pub panel_url: String,
}

/// The site's configuration, as set when the router was built.
fn cfg(cx: &Cx) -> Arc<SiteConfig> {
    context::<Arc<SiteConfig>>(cx).clone()
}

pub fn router(config: SiteConfig) -> Router {
    module_router!().discover().with(Arc::new(config)).build()
}

/// Renders a view to the HTML a response carries.
///
/// # Errors
///
/// The error of the failed render.
pub async fn html(cx: &Cx, page: impl View) -> TopcoatResult<Html<String>> {
    let content = page.single().await?;
    Ok(Html(content.render(cx)))
}

/// `GET /`
#[page]
async fn home(cx: &Cx) -> TopcoatResult<impl View> {
    let config = cfg(cx);
    Ok(view! { home_page(panel_url: &config.panel_url, public_url: &config.public_url) })
}
