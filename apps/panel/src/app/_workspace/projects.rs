//! `GET /projects` and `POST /projects`

mod project;

use crate::app::{_workspace::title, account, fail, panel, respond};
use crate::{
    Account, Arc, Deserialize, PageResult, Panel, StatusCode, auth,
    project::{Filter, project_catalog},
    records, redirect, safe_name,
    views::projects::{ProjectName, Projects, projects_page},
};
use rupicola_api::NewProject;
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, page, parse_query_params, response::Response, route},
    view::{View, view},
};

#[page(with(Metadata::new().title(title("Projects"))))]
async fn list(cx: &Cx) -> Result<impl View> {
    let filter: Filter = parse_query_params(cx)?;
    let data = load(cx, &filter).await.map_err(fail)?;
    Ok(view! { projects_page(data: &data) })
}

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<CreateProject>) -> Result<Response> {
    respond(cx, create_project(panel(cx), account(cx), form).await)
}

async fn load(cx: &Cx, filter: &Filter) -> PageResult<Projects> {
    let projects = project_catalog(cx).await?;
    let query = filter.q.clone().unwrap_or_default();
    let search = query.trim().to_lowercase();
    let empty_catalog = projects.is_empty();
    let total = projects.len();
    let visible: Vec<ProjectName> = projects
        .into_iter()
        .map(|(id, label)| ProjectName { id, label })
        .filter(|project| {
            search.is_empty()
                || project.id.contains(&search)
                || project.label.to_lowercase().contains(&search)
        })
        .collect();
    let mut summaries = BTreeMap::new();
    for ProjectName { id, .. } in &visible {
        if let Some(entry) = records::design(cx, id).await?.into_iter().next() {
            summaries.insert(
                id.clone(),
                format!("{} · {}", entry.entry.title, entry.entry.status.label()),
            );
        }
    }
    Ok(Projects {
        query,
        search,
        visible,
        total,
        empty_catalog,
        summaries,
    })
}

#[derive(Deserialize)]
struct CreateProject {
    csrf: String,
    id: String,
    label: String,
}

async fn create_project(
    panel: Arc<Panel>,
    account: Account,
    form: CreateProject,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    register_project(&panel, &account, &form.id, &form.label).await?;
    Ok(redirect(&format!("/projects/{}", form.id)))
}

async fn register_project(
    panel: &Panel,
    account: &Account,
    id: &str,
    label: &str,
) -> PageResult<()> {
    let label = label.trim();
    if !safe_name(id) || id.len() > 48 || label.is_empty() || label.len() > 400 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .create_project(
            &account.session,
            &NewProject {
                id: id.to_owned(),
                label: label.to_owned(),
            },
        )
        .await
        .map(|_| ())
        .map_err(|e| auth::api_error(&e))
}
