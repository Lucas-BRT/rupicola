//! `GET /projects/{project}/design`

mod edit;
mod preview;
mod proof;
mod record;
mod version;

use crate::app::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, PageResult, Panel, design_system,
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
    let all_assets = project.assets();
    let design = records::design(&panel, &account, &id).await?;
    let superseded = rupicola_core::design_record::superseded(&design);
    let tab = match filter.view.as_deref() {
        Some("system") => "system",
        Some("screens") => "screens",
        _ => "records",
    };
    let studio = design_system::load(&panel, &id, filter.version.as_deref()).await?;
    let cx = &Cx::default();
    let data = views::project::design::Design {
        base: &project.base,
        role: project.role,
        tab,
        records: &design,
        superseded: &superseded,
        all_assets: &all_assets,
        studio: &studio,
    };
    let body = views::project::design::render(cx, &account, data);
    views::project::frame(cx, &account, &panel, &project, Tab::Design, body).await
}
