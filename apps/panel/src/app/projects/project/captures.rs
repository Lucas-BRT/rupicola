//! `GET /projects/{project}/captures`

use crate::app::projects::project::Project;
use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, PageResult, Panel,
    project::{Context, Filter},
    records,
    views::{self, project::Tab},
};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Html, parse_query_params, path_param, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            parse_query_params(cx)?,
        )
        .await,
    )
}

/// The captures tab, which `runs`, `images` and `videos` also open with another view.
pub(super) async fn show(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    filter: Filter,
) -> PageResult<Html<String>> {
    let project = Context::load(&panel, &account, &id).await?;
    let related = project.related();
    let assets = project.assets();
    let problems = records::problems(&panel, &account, &id, &assets, &[]).await?;
    let cx = &Cx::default();
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
