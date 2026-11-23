//! `GET` and `POST /run/{capture_id}`: one capture and its review.

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form, module_param, parse_query_params, path_param, response::Response, route,
    },
};

module_param!(pub capture_id);

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        crate::detail(
            parse_query_params(cx)?,
            account(cx),
            panel(cx),
            path_param::<CaptureId>(cx).into(),
        )
        .await,
    )
}

#[route(POST)]
async fn inspect(cx: &Cx, Form(form): Form<pages::Review>) -> Result<Response> {
    respond(
        cx,
        pages::review(
            panel(cx),
            account(cx),
            path_param::<CaptureId>(cx).into(),
            form,
        )
        .await,
    )
}
