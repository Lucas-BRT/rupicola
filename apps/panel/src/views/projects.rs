use crate::Account;
use crate::components::{
    controls::{SelectOption, select_menu},
    display::empty,
    form::stack_form,
};
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
    /// Selected sort order key.
    pub sort: String,
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
        sort,
        search,
        visible,
        total,
        empty_catalog,
        summaries,
    } = data;
    let account = context::<Account>(cx);
    Ok(view! {
        <div id="projects-region">
            <div class="page-title studio-heading">
                <div>
                    <h1>"Projects"</h1>
                    <p class="muted" data-swap-meta="count">(visible.len()) " of " (total) (if *total == 1 { " project" } else { " projects" })</p>
                </div>
                search_form(query: query, sort: sort)
            </div>
            <div data-swap-body="cards">
                if visible.is_empty() {
                    empty(message: if search.is_empty() { "No projects yet." } else { "No projects match that search." })
                }
                <div class="project-index">
                    for ProjectName { id, label } in visible {
                        project_card(id: id, label: label, summary: summaries.get(id).map(String::as_str))
                    }
                </div>
            </div>
        </div>
        if account.owner {
            new_project(token: &account.csrf, open: *empty_catalog)
        }
    })
}

/// The project search box.
#[component]
async fn search_form(cx: &Cx, query: &str, sort: &str) -> Result<impl View> {
    let sort_options = [
        SelectOption::new("newest", "Newest first"),
        SelectOption::new("oldest", "Oldest first"),
        SelectOption::new("name", "Name A–Z"),
    ];
    Ok(view! {
        <form class="filters" method="get" action="/projects" data-swap-target="projects-region">
            <label for="q">"Search"</label>
            <input type="search" id="q" name="q" value=(query) maxlength="100" placeholder="Filter projects…">
            select_menu(
                id: "proj-sort",
                name: "sort",
                label: "Sort projects",
                selected: sort,
                options: &sort_options,
                auto_submit: true,
                compact: true,
            )
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
