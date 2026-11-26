use crate::components::{
    controls::{SegmentedOption, SelectOption, query_url, segmented_control, select_menu},
    display::{empty, moment},
};
use crate::{
    Run,
    project::{Filter, Proof},
};
use rupicola_api::{DesignKind, DesignStatus};
use topcoat::{
    Result,
    context::Cx,
    view::{Child, View, component, view},
};

/// A design record on the dashboard.
pub struct Activity {
    pub project: String,
    pub label: String,
    pub timestamp_ms: u64,
    pub title: String,
    pub kind: DesignKind,
    pub status: DesignStatus,
}

/// What the dashboard shows.
pub struct Dashboard {
    pub filter: Filter,
    pub activity: Vec<Activity>,
    pub latest_evidence: Option<Proof>,
    pub runs: Vec<Run>,
    /// Positions in `runs` of the captures the queue lists.
    pub queue: Vec<usize>,
}

/// The dashboard's outer wrapper around `child`, which holds what takes time to load.
#[component]
pub async fn dashboard_frame(cx: &Cx, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <div class="dash-page">
            (child)
        </div>
    })
}

fn state_href(state: &str, sort: &str) -> String {
    let sort_param = if sort == "newest" { "" } else { sort };
    query_url("/dashboard", &[("state", state), ("sort", sort_param)])
}

fn run_env_meta(run: &Run) -> String {
    let env = &run.capture.environment;
    let mut parts = Vec::new();
    if let (Some(w), Some(h)) = (env.width, env.height) {
        parts.push(format!("{w}×{h}"));
    }
    if let Some(scheme) = &env.color_scheme {
        parts.push(scheme.clone());
    }
    parts.join(" · ")
}

/// The dashboard's studio triage desk, once its data is loaded.
#[component]
pub async fn dashboard_board(cx: &Cx, data: &Dashboard) -> Result<impl View> {
    let active_state = data.filter.state.as_deref().unwrap_or("");
    let sort_key = data.filter.sort_key();
    let items: Vec<&Run> = data.queue.iter().map(|&index| &data.runs[index]).collect();

    Ok(view! {
        <header class="dash-head">
            <div class="dash-title-group">
                <h1>"Dashboard"</h1>
            </div>
        </header>
        <div class="dash-grid">
            capture_queue(active_state: active_state, sort_key: sort_key, items: &items)
            dashboard_rail(latest_evidence: data.latest_evidence.as_ref(), activity: &data.activity)
        </div>
    })
}

#[component]
async fn capture_queue(
    cx: &Cx,
    active_state: &str,
    sort_key: &str,
    items: &[&Run],
) -> Result<impl View> {
    let status_tabs = [
        ("", "All"),
        ("pending", "Needs review"),
        ("blocked", "Blocked"),
        ("reviewed", "Reviewed"),
    ]
    .into_iter()
    .map(|(value, label)| {
        SegmentedOption::new(label, state_href(value, sort_key), active_state == value)
    })
    .collect::<Vec<_>>();
    let sort_options = [
        SelectOption::new("newest", "Newest"),
        SelectOption::new("oldest", "Oldest"),
        SelectOption::new("scenario", "Scenario A–Z"),
    ];
    Ok(view! {
        <section id="dash-queue" class="dash-queue" aria-labelledby="dash-queue-heading">
            <div class="dash-queue-head">
                <div class="dash-queue-title">
                    <h2 id="dash-queue-heading">"Capture queue"</h2>
                    <span class="dash-count-pill" data-swap-meta="count">(items.len().to_string())</span>
                </div>
                <div class="dash-queue-controls">
                    segmented_control(
                        label: "Filter captures by status",
                        options: &status_tabs,
                        swap_target: "dash-queue",
                    )
                    <form class="dash-sort-form" method="get" action="/dashboard" data-swap-target="dash-queue">
                        <input type="hidden" name="state" value=(active_state) data-swap-meta="state">
                        select_menu(
                            id: "d-sort",
                            name: "sort",
                            label: "Sort captures",
                            selected: sort_key,
                            options: &sort_options,
                            auto_submit: true,
                            compact: true,
                        )
                        <noscript><button class="button button-ghost" type="submit">"Sort"</button></noscript>
                    </form>
                </div>
            </div>

            <div class="dash-queue-body" data-swap-body="rows">
                if items.is_empty() {
                    empty(message: "No captures match this filter.")
                } else {
                    <ol class="dash-run-list">
                        for run in items {
                            queue_row(run: run)
                        }
                    </ol>
                }
            </div>
        </section>
    })
}

#[component]
async fn queue_row(cx: &Cx, run: &Run) -> Result<impl View> {
    let status_class = if run.status == "blocked" {
        "dot-blocked"
    } else if run.inspection == "reviewed" {
        "dot-ok"
    } else {
        "dot-pending"
    };
    let status_text = if run.status == "blocked" {
        "Blocked"
    } else if run.inspection == "reviewed" {
        "Reviewed"
    } else {
        "Needs review"
    };
    let env_meta = run_env_meta(run);
    Ok(view! {
        <li>
            <a class="dash-run-row" href=(format!("/run/{}", run.key))>
                <span class=(format!("dash-dot {status_class}")) aria-hidden="true"></span>
                <span class="dash-run-main">
                    <span class="dash-run-name">(&run.scenario)</span>
                    <span class="dash-run-meta">
                        <span>(&run.project)</span>
                        if !env_meta.is_empty() {
                            <span aria-hidden="true">"·"</span>
                            <span>(env_meta)</span>
                        }
                    </span>
                </span>
                <span class="dash-run-right">
                    <span class=(format!("dash-status-label is-{status_class}"))>(status_text)</span>
                    moment(epoch_ms: run.created_at_ms)
                    <span class="dash-row-arrow" aria-hidden="true">"↗"</span>
                </span>
            </a>
        </li>
    })
}

#[component]
async fn dashboard_rail(
    cx: &Cx,
    latest_evidence: Option<&Proof>,
    activity: &[Activity],
) -> Result<impl View> {
    Ok(view! {
        <aside class="dash-rail" aria-label="Latest visual evidence and design log">
            if let Some(proof) = latest_evidence {
                <section class="dash-card dash-spotlight" aria-labelledby="dash-spotlight-heading">
                    <div class="dash-card-head">
                        <h2 id="dash-spotlight-heading">"Latest capture"</h2>
                        <a class="dash-quiet-link" href=(&proof.href)>"Inspect ↗"</a>
                    </div>
                    <a class="dash-proof-link" href=(&proof.href)>
                        <div class="dash-proof-frame">
                            <img src=(&proof.image) alt=(format!("Latest capture: {}", proof.scenario)) decoding="async">
                        </div>
                        <span class="dash-proof-caption">(&proof.scenario)</span>
                    </a>
                </section>
            }

            <section class="dash-card dash-activity-card" aria-labelledby="dash-activity-heading">
                <div class="dash-card-head">
                    <h2 id="dash-activity-heading">"Design log"</h2>
                </div>
                if activity.is_empty() {
                    <p class="muted dash-empty-note">"No design decisions recorded yet."</p>
                } else {
                    <ol class="dash-activity-list">
                        for entry in activity {
                            <li>
                                <a class="dash-activity-item" href=(format!("/projects/{}/design", entry.project)) data-nav-partial="">
                                    <span class="dash-activity-top">
                                        <span class="dash-activity-kind">(entry.kind.label())</span>
                                        moment(epoch_ms: entry.timestamp_ms)
                                    </span>
                                    <span class="dash-activity-title">(&entry.title)</span>
                                    <span class="dash-activity-meta">(&entry.label) " · " (entry.status.label())</span>
                                </a>
                            </li>
                        }
                    </ol>
                }
            </section>
        </aside>
    })
}
