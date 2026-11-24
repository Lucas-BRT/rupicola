//! The stylesheet, the script and the install script. Their paths carry a file extension, so
//! they are written out instead of taken from the module name.

use super::cfg;
use http::{StatusCode, header};
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::{Body, response::Response, route},
};

const SITE_CSS: &str = include_str!("../../assets/site.css");
const SITE_JS: &str = include_str!("../../assets/site.js");
const INSTALL_SH: &str = include_str!("../../assets/install.sh");
/// Text in `install.sh` replaced by the site's public URL when the script is served.
const PUBLIC_URL_PLACEHOLDER: &str = "@PUBLIC_URL@";

#[route(GET "/site.css")]
async fn css(cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(SITE_CSS))
        .expect("valid css response"))
}

#[route(GET "/site.js")]
async fn js(cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(SITE_JS))
        .expect("valid js response"))
}

#[route(GET "/install.sh")]
async fn install_script(cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=300")
        .body(Body::from(
            INSTALL_SH.replace(PUBLIC_URL_PLACEHOLDER, &cfg(cx).public_url),
        ))
        .expect("valid install.sh response"))
}
