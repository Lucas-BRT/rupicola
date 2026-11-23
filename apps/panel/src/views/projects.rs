use crate::Account;
use crate::components::{display::empty, form::stack_form};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// A project's id and the label it goes by.
pub struct ProjectName {
    pub id: String,
    pub label: String,
}

/// What the project list shows.
pub struct Projects {
    /// The search the visitor typed, as typed.
    pub query: String,
    /// The search, trimmed and lower-cased.
    pub search: String,
    /// The projects that match.
    pub visible: Vec<ProjectName>,
    pub total: usize,
    pub empty_catalog: bool,
    pub summaries: BTreeMap<String, String>,
}

/// The project list's content.
#[component]
pub async fn projects_page(cx: &Cx, data: &Projects) -> Result<impl View> {
    let Projects {
        query,
        search,
        visible,
        total,
        empty_catalog,
        summaries,
    } = data;
    let account = context::<Account>(cx);
    Ok(view! {
        <div class="page-title studio-heading">
            <div>
                <h1>"Projects"</h1>
                <p class="muted">(visible.len()) " of " (total) (if *total == 1 { " project" } else { " projects" })</p>
            </div>
            search_form(query: query)
        </div>
        if visible.is_empty() {
            empty(message: if search.is_empty() { "No projects yet." } else { "No projects match that search." })
        }
        <div class="project-index">
            for ProjectName { id, label } in visible {
                project_card(id: id, label: label, summary: summaries.get(id).map(String::as_str))
            }
        </div>
        if account.owner {
            new_project(token: &account.csrf, open: *empty_catalog)
        }
    })
}

/// The project search box.
#[component]
async fn search_form(cx: &Cx, query: &str) -> Result<impl View> {
    Ok(view! {
        <form class="filters" method="get" action="/projects">
            <label for="q">"Search"</label>
            <input type="search" id="q" name="q" value=(query) maxlength="100">
            <button type="submit" class="button">"Search"</button>
        </form>
    })
}

/// One project in the index.
#[component]
async fn project_card(cx: &Cx, id: &str, label: &str, summary: Option<&str>) -> Result<impl View> {
    Ok(view! {
        <a class="project-card" href=(format!("/projects/{id}"))>
            <div class="project-emblem" aria-hidden="true">"▱"</div>
            <div><h2>(label)</h2>if id != label { <small>(id)</small> }</div>
            <span class="project-count">(summary.unwrap_or("No design records yet"))</span>
            <span class="arrow" aria-hidden="true">"↗"</span>
        </a>
    })
}

/// The form that creates a project; `open` unfolds it when there is no project yet.
#[component]
async fn new_project(cx: &Cx, token: &str, open: bool) -> Result<impl View> {
    Ok(view! {
        <details class="surface new-project" if open { open="" }>
            <summary>"Create a project"</summary>
            stack_form(action: "/projects", token: token,
                <label for="label">"Project name"</label>
                <input id="label" name="label" required="" maxlength="100">
                <label for="project-id">"Project ID"</label>
                <input id="project-id" name="id" required="" maxlength="48" pattern=("[a-zA-Z0-9_\\-]+") aria-describedby="id-help">
                <small id="id-help">"Use the same ID in capture scenarios and publication commands."</small>
                <button type="submit" class="button primary">"Create project"</button>
            )
        </details>
    })
}
