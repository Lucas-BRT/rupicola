//! `GET /docs` and `GET /docs/{*slug}`

use super::cfg;
use crate::{content::docs, views};
use http::{StatusCode, header};
use topcoat::{
    Result as TopcoatResult,
    context::Cx,
    router::{
        Body,
        content::Html,
        path_param,
        response::{IntoResponse, Response},
        route,
    },
};

path_param!(*slug);

#[route(GET)]
async fn index(cx: &Cx) -> TopcoatResult<Response> {
    Html(views::docs(&docs::DOCS[0], &cfg(cx).panel_url)).into_response(cx)
}

#[route(GET "/docs/{*slug}")]
async fn page(cx: &Cx) -> TopcoatResult<Response> {
    let slug = path_param::<Slug>(cx).collect::<Vec<_>>().join("/");
    match docs::find_doc(&slug) {
        Some(doc) => Html(views::docs(doc, &cfg(cx).panel_url)).into_response(cx),
        None => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("Document not found"))
            .expect("valid 404 response")),
    }
}
