use crate::components::{
    display::{counts, kind_label, link_button, metric, moment, proof_card, section_head},
    layout::{Nav, page_title, workspace},
};
use crate::{Account, PageResult, Panel, auth, project::Filter};
use rupicola_core::design_record::{Kind, Status};
use topcoat::{
    context::Cx,
    router::content::Html,
    view::{Child, view},
};

/// A design record on the dashboard: project id, project name, time, title, kind, status.
pub type Activity<'a> = (&'a String, &'a String, u64, String, Kind, Status);

/// What the dashboard shows.
pub struct Dashboard<'a> {
    pub filter: &'a Filter,
    pub project_count: usize,
    pub activity: &'a [Activity<'a>],
    pub pending: usize,
    pub blocked: usize,
    pub latest_evidence: Option<(String, String, &'a String)>,
    pub list: Child<'a>,
}

pub async fn render(
    cx: &Cx,
    account: &Account,
    panel: &Panel,
    data: Dashboard<'_>,
) -> PageResult<Html<String>> {
    let Dashboard {
        filter,
        project_count,
        activity,
        pending,
        blocked,
        latest_evidence,
        list,
    } = data;
    let content = view! {
        workspace(title: "Dashboard", account: account, panel: panel, active: Nav::Dashboard,
            page_title(title: "Dashboard", class: "studio-heading",
                link_button(href: "/projects", primary: true, "Browse projects")
            )
            <div class="studio-board">
                <div class="studio-evidence">
                    if let Some((href, image, scenario)) = &latest_evidence {
                        proof_card(href: href, image: image, scenario: scenario,
                            <h2>"Latest evidence"</h2>
                        )
                    }
                    counts(label: "Workspace activity",
                        metric(href: "/?state=pending", value: pending, featured: pending > 0, "Needs review")
                        metric(href: "/?state=blocked", value: blocked, "Blocked captures")
                        metric(href: "/projects", value: project_count, "Projects")
                    )
                    <section class="surface studio-queue">
                        section_head(title: "Capture queue",
                            <form class="filters" method="get" action="/">
                                <label for="state">"Status"</label>
                                <select id="state" name="state">
                                    for (value, text) in [("", "All captures"), ("pending", "Needs review"), ("blocked", "Blocked"), ("reviewed", "Reviewed")] {
                                        <option value=(value) if filter.state.as_deref().unwrap_or("") == value { selected="" }>(text)</option>
                                    }
                                </select>
                                <button class="button" type="submit">"Filter"</button>
                            </form>
                        )
                        (list)
                    </section>
                </div>
                <section class="studio-ledger">
                    <h2>"Recent design activity"</h2>
                    if activity.is_empty() {
                        <p>"No design records yet."</p>
                        <a href="/projects">"Open projects"</a>
                    } else {
                        <ol class="studio-activity">
                            for (id, label, timestamp, title, kind, status) in activity {
                                <li>
                                    kind_label((kind.label()))
                                    <a class="studio-record" href=(format!("/projects/{id}/design"))>(title)</a>
                                    <p class="muted">(label) " · " (status.label()) " · " moment(epoch_ms: *timestamp)</p>
                                </li>
                            }
                        </ol>
                    }
                </section>
            </div>
        )
    };
    auth::page(cx, content).await
}
