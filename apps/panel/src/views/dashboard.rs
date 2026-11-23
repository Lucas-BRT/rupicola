use crate::components::{
    display::{
        counts, kind_label, link_button, metric, moment, proof_card, run_list, section_head,
    },
    layout::page_title,
};
use crate::{Run, project::Filter};
use rupicola_core::design_record::{Kind, Status};
use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// A design record on the dashboard.
pub struct Activity {
    pub project: String,
    pub label: String,
    pub timestamp_ms: u64,
    pub title: String,
    pub kind: Kind,
    pub status: Status,
}

/// What the dashboard shows.
pub struct Dashboard {
    pub filter: Filter,
    pub project_count: usize,
    pub activity: Vec<Activity>,
    pub pending: usize,
    pub blocked: usize,
    /// Link, image and scenario of the newest verified capture.
    pub latest_evidence: Option<(String, String, String)>,
    pub runs: Vec<Run>,
    /// Positions in `runs` of the captures the queue lists.
    pub queue: Vec<usize>,
}

/// The dashboard's content.
#[component]
pub async fn dashboard_page(cx: &Cx, data: &Dashboard) -> Result<impl View> {
    let Dashboard {
        filter,
        project_count,
        activity,
        pending,
        blocked,
        latest_evidence,
        runs,
        queue,
    } = data;
    let items: Vec<&Run> = queue.iter().map(|&index| &runs[index]).collect();
    Ok(view! {
        page_title(title: "Dashboard", class: "studio-heading",
            link_button(href: "/projects", primary: true, "Browse projects")
        )
        <div class="studio-board">
            <div class="studio-evidence">
                if let Some((href, image, scenario)) = latest_evidence {
                    proof_card(href: href, image: image, scenario: scenario,
                        <h2>"Latest evidence"</h2>
                    )
                }
                counts(label: "Workspace activity",
                    metric(href: "/dashboard?state=pending", value: *pending, featured: *pending > 0, "Needs review")
                    metric(href: "/dashboard?state=blocked", value: *blocked, "Blocked captures")
                    metric(href: "/projects", value: *project_count, "Projects")
                )
                <section class="surface studio-queue">
                    section_head(title: "Capture queue",
                        <form class="filters" method="get" action="/dashboard">
                            <label for="state">"Status"</label>
                            <select id="state" name="state">
                                for (value, text) in [("", "All captures"), ("pending", "Needs review"), ("blocked", "Blocked"), ("reviewed", "Reviewed")] {
                                    <option value=(value) if filter.state.as_deref().unwrap_or("") == value { selected="" }>(text)</option>
                                }
                            </select>
                            <button class="button" type="submit">"Filter"</button>
                        </form>
                    )
                    run_list(runs: &items, message: "No matching captures.")
                </section>
            </div>
            <section class="studio-ledger">
                <h2>"Recent design activity"</h2>
                if activity.is_empty() {
                    <p>"No design records yet."</p>
                    <a href="/projects">"Open projects"</a>
                } else {
                    <ol class="studio-activity">
                        for entry in activity {
                            <li>
                                kind_label((entry.kind.label()))
                                <a class="studio-record" href=(format!("/projects/{}/design", entry.project))>(&entry.title)</a>
                                <p class="muted">(&entry.label) " · " (entry.status.label()) " · " moment(epoch_ms: entry.timestamp_ms)</p>
                            </li>
                        }
                    </ol>
                }
            </section>
        </div>
    })
}
