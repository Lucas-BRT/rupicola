//! `GET /projects/{project}/fixes`

mod create;
mod fix;

use crate::app::projects::project::project as project_param;
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
    router::{content::Html, parse_query_params, response::Response, route},
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(
            panel(cx),
            account(cx),
            project_param(cx).into(),
            parse_query_params(cx)?,
        )
        .await,
    )
}

async fn show(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    filter: Filter,
) -> PageResult<Html<String>> {
    let project = Context::load(&panel, &account, &id).await?;
    let assets = project.assets();
    let fixes = records::fixes(&panel, &account, &id).await?;
    let problems = records::problems(&panel, &account, &id, &assets, &fixes).await?;
    let cx = &Cx::default();
    let body = views::project::render_fixes(
        cx,
        &account,
        project.role,
        &id,
        &project.base,
        &filter,
        &fixes,
        &problems,
        &assets,
    );
    views::project::frame(cx, &account, &panel, &project, Tab::Fixes, body).await
}
