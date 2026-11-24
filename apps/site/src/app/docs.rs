//! `GET /docs` and `GET /docs/{*slug}`

use super::{cfg, html};
use crate::{content::docs, views::docs_page};
use http::{StatusCode, header};
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::{
        Body, path_param,
        response::{IntoResponse, Response},
        route,
    },
    view::view,
};

path_param!(*slug);

#[route(GET)]
async fn index(cx: &Cx) -> TopcoatResult<Response> {
    let config = cfg(cx);
    let body = view! { docs_page(doc: &docs::DOCS[0], panel_url: &config.panel_url) };
    html(cx, body).await?.into_response(cx)
}

#[route(GET "/docs/{*slug}")]
async fn page(cx: &Cx) -> TopcoatResult<Response> {
    let slug = slug(cx).collect::<Vec<_>>().join("/");
    let config = cfg(cx);
    match docs::find_doc(&slug) {
        Some(doc) => {
            let body = view! { docs_page(doc: doc, panel_url: &config.panel_url) };
            html(cx, body).await?.into_response(cx)
        }
        None => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("Document not found"))
            .expect("valid 404 response")),
    }
}
