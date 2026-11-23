//! The stylesheet and the script. Their paths carry a file extension, so they are written
//! out instead of taken from the module name.

use http::header;
use topcoat::{Result, context::Cx, router::route};

#[route(GET "/panel.css")]
async fn styles(cx: &Cx) -> Result<([(http::HeaderName, &'static str); 1], &'static str)> {
    Ok((
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!(concat!(env!("OUT_DIR"), "/tailwind.css")),
    ))
}

#[route(GET "/panel.js")]
async fn script(cx: &Cx) -> Result<([(http::HeaderName, &'static str); 1], &'static str)> {
    Ok((
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../panel.js"),
    ))
}
