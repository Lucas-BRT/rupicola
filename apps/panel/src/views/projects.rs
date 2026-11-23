use crate::{Account, PageResult, Panel, auth, project::Filter};
use std::collections::BTreeMap;
use topcoat::{context::Cx, router::content::Html, view::view};

/// What the project list shows.
pub struct Projects<'a> {
    pub filter: &'a Filter,
    pub search: &'a str,
    pub visible: Vec<(&'a String, &'a String)>,
    pub total: usize,
    pub empty_catalog: bool,
    pub summaries: &'a BTreeMap<String, String>,
}

pub async fn render(
    cx: &Cx,
    account: &Account,
    panel: &Panel,
    data: Projects<'_>,
) -> PageResult<Html<String>> {
    let Projects {
        filter,
        search,
        visible,
        total: total_projects,
        empty_catalog,
        summaries,
    } = data;
    let content = shell!(cx, "Projects", account, panel, "projects", {
        <div class="page-title studio-heading"><div><h1>"Projects"</h1><p class="muted">(visible.len()) " of " (total_projects) (if total_projects == 1 { " project" } else { " projects" })</p></div>
            <form class="filters" method="get" action="/projects">
                <label for="q">"Search"</label><input type="search" id="q" name="q" value=(filter.q.as_deref().unwrap_or("")) maxlength="100">
                <button type="submit" class="button">"Search"</button>
            </form>
        </div>
        if visible.is_empty() { <div class="empty"><p>(if search.is_empty() { "No projects yet." } else { "No projects match that search." })</p></div> }
        <div class="project-index">
            for (id, label) in visible {
                <a class="project-card" href=(format!("/projects/{id}"))>
                    <div class="project-emblem" aria-hidden="true">"▱"</div>
                    <div><h2>(label)</h2>if id != label { <small>(id)</small> }</div>
                    <span class="project-count">
                        if let Some(summary) = summaries.get(id) {
                            (summary)
                        } else { "No design records yet" }
                    </span>
                    <span class="arrow" aria-hidden="true">"↗"</span>
                </a>
            }
        </div>
        if account.owner {
            <details class="surface new-project" if empty_catalog { open="" }>
                <summary>"Create a project"</summary>
                <form class="stack-form" method="post" action="/projects">
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="label">"Project name"</label><input id="label" name="label" required="" maxlength="100">
                    <label for="project-id">"Project ID"</label><input id="project-id" name="id" required="" maxlength="48" pattern=("[a-zA-Z0-9_\\-]+") aria-describedby="id-help">
                    <small id="id-help">"Use the same ID in capture scenarios and publication commands."</small>
                    <button type="submit" class="button primary">"Create project"</button>
                </form>
            </details>
        }
    });
    auth::page(cx, content).await
}
