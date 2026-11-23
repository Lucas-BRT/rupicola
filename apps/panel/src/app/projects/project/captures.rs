//! `GET /projects/{project}/captures`

use crate::app::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{
    PageResult,
    project::{Context, Filter},
    records,
    views::{self, project::Tab},
};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Html, parse_query_params, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(cx, project_param(cx).into(), parse_query_params(cx)?).await,
    )
}

/// The captures tab, which `runs`, `images` and `videos` also open with another view.
pub(super) async fn show(cx: &Cx, id: String, filter: Filter) -> PageResult<Html<String>> {
    let panel = panel(cx);
    let account = account(cx);
    let project = Context::load(cx, &id).await?;
    let related = project.related();
    let assets = project.assets();
    let problems = records::problems(cx, &id, &assets, &[]).await?;
    let body = views::project::render_captures(
        cx,
        &account,
        project.role,
        &id,
        &project.base,
        &filter,
        &related,
        &assets,
        &problems,
    );
    views::project::frame(cx, &account, &panel, &project, Tab::Captures, body).await
}
