//! `GET` and `POST /projects/{project}/settings`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, pages};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, parse_query_params, path_param, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::project_section(
            panel(cx),
            account(cx),
            (path_param::<Project>(cx).into(), "settings".into()),
            parse_query_params(cx)?,
        )
        .await,
    )
}

#[route(POST)]
async fn change_access(cx: &Cx, Form(form): Form<pages::ProjectAction>) -> Result<Response> {
    let path = (path_param::<Project>(cx).into(), "settings".into());
    respond(
        cx,
        pages::project_action(panel(cx), account(cx), path, form).await,
    )
}
