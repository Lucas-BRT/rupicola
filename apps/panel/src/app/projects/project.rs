//! `GET /projects/{project}`: the overview; each child module is a tab of the project.

mod captures;
mod design;
mod findings;
mod fixes;
mod images;
mod problems;
mod runs;
mod settings;
mod videos;

use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, PageResult, Panel, auth, media,
    project::{Context, Filter, pick_hero_checkpoint},
    records,
    views::{self, project::Tab, project::overview::Overview},
};
use rupicola_core::design_record;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Html, module_param, path_param, response::Response, route},
};

module_param!(pub project);

#[route(GET)]
async fn overview(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(panel(cx), account(cx), path_param::<Project>(cx).into()).await,
    )
}

/// Recent captures listed on the overview.
const RECENT_CAPTURES: usize = 8;

async fn show(panel: Arc<Panel>, account: Account, id: String) -> PageResult<Html<String>> {
    let project = Context::load(&panel, &account, &id).await?;
    let related = project.related();
    let all_assets = project.assets();
    let design = records::design(&panel, &account, &id).await?;
    let superseded = design_record::superseded(&design);
    let current = design
        .iter()
        .filter(|entry| !superseded.contains(entry.id.as_str()))
        .count();
    let fix_count = panel
        .api
        .fixes(&account.session, &id)
        .await
        .map_err(|e| auth::api_error(&e))?
        .len();
    let open_problems_count = panel
        .api
        .problems(&account.session, &id)
        .await
        .map_err(|e| auth::api_error(&e))?
        .iter()
        .filter(|problem| {
            matches!(
                problem.status,
                rupicola_api::ProblemStatus::Open | rupicola_api::ProblemStatus::NeedsReprocess
            )
        })
        .count();
    let filter = Filter::default();
    let selected: Vec<_> = related
        .iter()
        .copied()
        .filter(|run| filter.accepts(run))
        .take(RECENT_CAPTURES)
        .collect();
    let latest_project_evidence = related
        .iter()
        .filter(|run| run.status == "complete")
        .find_map(|run| {
            let checkpoint = pick_hero_checkpoint(&run.manifest.checkpoints)?;
            Some((
                format!("/run/{}", run.key),
                media::url(&checkpoint.path),
                run.scenario.clone(),
            ))
        });
    let cx = &Cx::default();
    let list = views::run_list(
        cx,
        &selected,
        "No matching captures. Send a capture with this project's ID.",
    );
    let data = Overview {
        base: &project.base,
        latest_project_evidence,
        records: &design,
        superseded: &superseded,
        current,
        related: &related,
        all_assets: &all_assets,
        open_problems_count,
        fix_count,
        list,
    };
    let body = views::project::overview::render(cx, data);
    views::project::frame(cx, &account, &panel, &project, Tab::Overview, body).await
}
