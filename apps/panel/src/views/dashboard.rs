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
    let content = shell!(cx, "Dashboard", account, panel, "dashboard", {
        <header class="page-title studio-heading"><h1>"Dashboard"</h1><a class="button primary" href="/projects">"Browse projects"</a></header>
        <div class="studio-board">
        <div class="studio-evidence">
        if let Some((href, image, scenario)) = &latest_evidence {
            <section class="studio-proof">
                <h2>"Latest evidence"</h2>
                <a href=(href)>
                    <div class="studio-proof-image"><img src=(image) alt="" decoding="async"></div>
                    <span>(scenario) <span aria-hidden="true">"↗"</span></span>
                </a>
            </section>
        }
        <section class="studio-counts" aria-label="Workspace activity">
            <a class=(if pending > 0 { "metric featured" } else { "metric" }) href="/?state=pending"><small>"Needs review"</small><strong>(pending)</strong><span aria-hidden="true">"↗"</span></a>
            <a class="metric" href="/?state=blocked"><small>"Blocked captures"</small><strong>(blocked)</strong><span aria-hidden="true">"↗"</span></a>
            <a class="metric" href="/projects"><small>"Projects"</small><strong>(project_count)</strong><span aria-hidden="true">"↗"</span></a>
        </section>
        <section class="surface studio-queue">
            <div class="section-head">
                <h2>"Capture queue"</h2>
                <form class="filters" method="get" action="/">
                    <label for="state">"Status"</label>
                    <select id="state" name="state">
                        for (value, text) in [("", "All captures"), ("pending", "Needs review"), ("blocked", "Blocked"), ("reviewed", "Reviewed")] {
                            <option value=(value) if filter.state.as_deref().unwrap_or("") == value { selected="" }>(text)</option>
                        }
                    </select>
                    <button class="button" type="submit">"Filter"</button>
                </form>
            </div>
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
                        <li><small class="studio-kind">(kind.label())</small>
                            <a class="studio-record" href=(format!("/projects/{id}/design"))>(title)</a>
                            <p class="muted">(label) " · " (status.label()) " · "
                                <time data-epoch-ms=(timestamp.to_string())></time>
                            </p>
                        </li>
                    }
                </ol>
            }
        </section>
        </div>
    });
    auth::page(cx, content).await
}
