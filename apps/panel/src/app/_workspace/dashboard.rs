//! `GET /dashboard`

use crate::app::{_workspace::title, fail};
use crate::{
    PageResult,
    project::{self, Filter, Proof},
    records,
    views::dashboard::{Activity, Dashboard, dashboard_board, dashboard_frame},
};
use rupicola_core::design_record;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, component, error_boundary, view},
};

/// Captures listed in the queue.
const QUEUE_LEN: usize = 30;
/// Design records listed as recent activity.
const ACTIVITY_LEN: usize = 8;

#[page(with(Metadata::new().title(title("Dashboard"))))]
async fn dashboard(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    Ok(view! {
        dashboard_frame(
            error_boundary(
                fallback: |_| Ok(view! {
                    <p class="notice" role="alert">"The captures could not be loaded. Try again in a moment."</p>
                }),
                loaded_board(filter: filter)
            )
        )
    })
}

/// The board, loaded apart from the heading so that a failed read leaves the page standing.
#[component]
async fn loaded_board(cx: &Cx, filter: Filter) -> Result<impl View> {
    let data = load(cx, filter).await.map_err(fail)?;
    Ok(view! { dashboard_board(data: &data) })
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
    let latest_evidence = Proof::newest(&runs);
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
