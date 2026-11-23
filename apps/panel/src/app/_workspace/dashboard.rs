//! `GET /dashboard`

use crate::app::{_workspace::title, fail};
use crate::{
    PageResult, media,
    project::{self, Filter, pick_hero_checkpoint},
    records,
    views::dashboard::{Activity, Dashboard, dashboard_page},
};
use rupicola_core::design_record;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, view},
};

/// Captures listed in the queue.
const QUEUE_LEN: usize = 30;
/// Design records listed as recent activity.
const ACTIVITY_LEN: usize = 8;

#[page(with(Metadata::new().title(title("Dashboard"))))]
async fn dashboard(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    let data = load(cx, filter).await.map_err(fail)?;
    Ok(view! { dashboard_page(data: &data) })
}

async fn load(cx: &Cx, filter: Filter) -> PageResult<Dashboard> {
    let runs = project::visible_runs(cx).await?;
    let projects = project::catalog(cx, &runs).await?;
    let mut activity: Vec<Activity> = Vec::new();
    for (id, label) in &projects {
        let records = records::design(cx, id).await?;
        let superseded = design_record::superseded(&records);
        activity.extend(
            records
                .iter()
                .filter(|entry| !superseded.contains(entry.id.as_str()))
                .take(ACTIVITY_LEN)
                .map(|entry| Activity {
                    project: id.clone(),
                    label: label.clone(),
                    timestamp_ms: entry.timestamp_ms,
                    title: entry.title.clone(),
                    kind: entry.kind,
                    status: entry.status,
                }),
        );
    }
    activity.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    activity.truncate(ACTIVITY_LEN);
    let pending = runs
        .iter()
        .filter(|r| r.status == "complete" && r.inspection == "pending")
        .count();
    let blocked = runs.iter().filter(|r| r.status == "blocked").count();
    let latest_evidence = runs
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
    let queue = runs
        .iter()
        .enumerate()
        .filter(|(_, r)| filter.accepts(r))
        .map(|(index, _)| index)
        .take(QUEUE_LEN)
        .collect();
    Ok(Dashboard {
        filter,
        project_count: projects.len(),
        activity,
        pending,
        blocked,
        latest_evidence,
        runs,
        queue,
    })
}
