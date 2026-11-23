//! The site's routes. Each module below this one is a path segment (`install` serves
//! `/install`); a handler only reads the request and hands data to a view.

mod architecture;
mod assets;
mod docs;
mod healthz;
mod install;

use crate::views;
use std::sync::Arc;
use topcoat::{
    Result as TopcoatResult,
    context::{Cx, app_context},
    router::{
        Router, RouterBuilderDiscoverExt, content::Html, module_router, response::IntoResponse,
        response::Response, route,
    },
};

/// Where the site and the review panel live, for links and the install script.
#[derive(Clone)]
pub struct SiteConfig {
    pub public_url: String,
    pub panel_url: String,
}

/// The site's configuration, as set when the router was built.
fn cfg(cx: &Cx) -> Arc<SiteConfig> {
    app_context::<Arc<SiteConfig>>(cx).clone()
}

pub fn router(config: SiteConfig) -> Router {
    module_router!()
        .discover()
        .app_context(Arc::new(config))
        .build()
}

/// `GET /`
#[route(GET)]
async fn home(cx: &Cx) -> TopcoatResult<Response> {
    let config = cfg(cx);
    Html(views::home(&config.panel_url, &config.public_url)).into_response(cx)
}
