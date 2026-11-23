//! `GET /projects` and `POST /projects`

mod project;

use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, Deserialize, PageResult, Panel, StatusCode, auth,
    project::{Filter, catalog, visible_runs},
    records, redirect, safe_name,
    views::{self, projects::Projects},
};
use rupicola_api::NewProject;
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Html},
        parse_query_params,
        response::Response,
        route,
    },
};

#[route(GET)]
async fn list(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(panel(cx), account(cx), parse_query_params(cx)?).await,
    )
}

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<CreateProject>) -> Result<Response> {
    respond(cx, create_project(panel(cx), account(cx), form).await)
}

async fn show(panel: Arc<Panel>, account: Account, filter: Filter) -> PageResult<Html<String>> {
    let runs = visible_runs(&panel, &account).await?;
    let projects = catalog(&panel, &account, &runs).await?;
    let search = filter.q.as_deref().unwrap_or("").trim().to_lowercase();
    let empty_catalog = projects.is_empty();
    let total_projects = projects.len();
    let visible: Vec<_> = projects
        .iter()
        .filter(|(id, label)| {
            search.is_empty() || id.contains(&search) || label.to_lowercase().contains(&search)
        })
        .collect();
    let mut summaries = BTreeMap::new();
    for (id, _) in &visible {
        if let Some(entry) = records::design(&panel, &account, id)
            .await?
            .into_iter()
            .next()
        {
            summaries.insert(
                (*id).clone(),
                format!("{} · {}", entry.title, entry.status.label()),
            );
        }
    }
    let cx = &Cx::default();
    let data = Projects {
        filter: &filter,
        search: &search,
        visible,
        total: total_projects,
        empty_catalog,
        summaries: &summaries,
    };
    views::projects::render(cx, &account, &panel, data).await
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
