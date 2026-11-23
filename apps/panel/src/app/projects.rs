//! `GET /projects` and `POST /projects`

mod project;

use crate::app::{account, panel, respond};
use crate::pages;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, parse_query_params, response::Response, route},
};

#[route(GET)]
async fn list(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::projects(panel(cx), account(cx), parse_query_params(cx)?).await,
    )
}

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<pages::CreateProject>) -> Result<Response> {
    respond(
        cx,
        pages::create_project(panel(cx), account(cx), form).await,
    )
}
