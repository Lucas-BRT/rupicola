//! `GET /dashboard`

use crate::app::{account, panel, respond};
use crate::components::display::run_list;
use crate::{
    PageResult, media,
    project::{self, Filter, pick_hero_checkpoint},
    records,
    views::{self, dashboard::Dashboard},
};
use rupicola_core::design_record;
use topcoat::view::{Child, view};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Html, parse_query_params, response::Response, route},
};

#[route(GET)]
async fn dashboard(cx: &Cx) -> Result<Response> {
    respond(cx, show(cx, parse_query_params(cx)?).await)
}

/// Captures listed in the queue.
const QUEUE_LEN: usize = 30;
/// Design records listed as recent activity.
const ACTIVITY_LEN: usize = 8;

async fn show(cx: &Cx, filter: Filter) -> PageResult<Html<String>> {
    let panel = panel(cx);
    let account = account(cx);
    let runs = project::visible_runs(cx).await?;
    let projects = project::catalog(cx, &runs).await?;
    let project_count = projects.len();
    let mut activity = Vec::new();
    for (id, label) in &projects {
        let records = records::design(cx, id).await?;
        let superseded = design_record::superseded(&records);
        activity.extend(
            records
                .iter()
                .filter(|entry| !superseded.contains(entry.id.as_str()))
                .take(ACTIVITY_LEN)
                .map(|entry| {
                    (
                        id,
                        label,
                        entry.timestamp_ms,
                        entry.title.clone(),
                        entry.kind,
                        entry.status,
                    )
                }),
        );
    }
    activity.sort_by_key(|entry| std::cmp::Reverse(entry.2));
    activity.truncate(ACTIVITY_LEN);
    let pending = runs
        .iter()
        .filter(|r| r.status == "complete" && r.inspection == "pending")
        .count();
    let blocked = runs.iter().filter(|r| r.status == "blocked").count();
    let items: Vec<_> = runs
        .iter()
        .filter(|r| filter.accepts(r))
        .take(QUEUE_LEN)
        .collect();
    let latest_evidence = runs
        .iter()
        .filter(|run| run.status == "complete")
        .find_map(|run| {
            let checkpoint = pick_hero_checkpoint(&run.manifest.checkpoints)?;
            Some((
                format!("/run/{}", run.key),
                media::url(&checkpoint.path),
                &run.scenario,
            ))
        });
    let list = Child::new(view! { run_list(runs: &items, message: "No matching captures.") });
    let data = Dashboard {
        filter: &filter,
        project_count,
        activity: &activity,
        pending,
        blocked,
        latest_evidence,
        list,
    };
    views::dashboard::render(cx, &account, &panel, data).await
}
