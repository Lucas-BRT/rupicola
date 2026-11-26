//! `GET /dashboard`

use crate::app::{_workspace::title, fail};
use crate::{
    PageResult,
    components::skeleton::dashboard_skeleton,
    project::{self, Filter, Proof},
    records,
    views::dashboard::{Activity, Dashboard, dashboard_board, dashboard_frame},
};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{page, parse_query_params},
    view::{View, component, error_boundary, suspense, view},
};

/// Captures listed in the queue.
const QUEUE_LEN: usize = 10;
/// Design records listed as recent activity.
const ACTIVITY_LEN: usize = 8;

#[page(with(Metadata::new().title(title("Dashboard"))))]
async fn dashboard(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    Ok(view! {
        dashboard_frame(
            suspense(
                fallback: view! { dashboard_skeleton() },
                error_boundary(
                    fallback: |_| Ok(view! {
                        <p class="notice" role="alert">"The captures could not be loaded. Try again in a moment."</p>
                    }),
                    loaded_board(filter: filter)
                )
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
        let superseded = records::superseded_design(&records);
        activity.extend(
            records
                .iter()
                .filter(|entry| !superseded.contains(entry.id.as_str()))
                .take(ACTIVITY_LEN)
                .map(|entry| Activity {
                    project: id.clone(),
                    label: label.clone(),
                    timestamp_ms: records::millis(entry.created_at),
                    title: entry.entry.title.clone(),
                    kind: entry.entry.kind,
                    status: entry.entry.status,
                }),
        );
    }
    activity.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    activity.truncate(ACTIVITY_LEN);
    let latest_evidence = Proof::newest(&runs);
    let mut matching: Vec<(usize, &crate::Run)> = runs
        .iter()
        .enumerate()
        .filter(|(_, r)| filter.accepts(r))
        .collect();
    match filter.sort_key() {
        "oldest" => matching.sort_by_key(|(_, r)| r.created_at_ms),
        "scenario" => matching.sort_by(|(_, a), (_, b)| {
            a.scenario
                .to_lowercase()
                .cmp(&b.scenario.to_lowercase())
                .then_with(|| b.created_at_ms.cmp(&a.created_at_ms))
        }),
        _ => matching.sort_by_key(|(_, r)| std::cmp::Reverse(r.created_at_ms)),
    }
    let queue = matching
        .into_iter()
        .map(|(index, _)| index)
        .take(QUEUE_LEN)
        .collect();
    Ok(Dashboard {
        filter,
        activity,
        latest_evidence,
        runs,
        queue,
    })
}
