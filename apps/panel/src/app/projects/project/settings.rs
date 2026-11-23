//! `GET` and `POST /projects/{project}/settings`

use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, Deserialize, PageResult, Panel, StatusCode,
    app::projects::project::Project,
    auth,
    auth::Role,
    db_error,
    project::{Context, access_lists},
    redirect, safe_name,
    views::credential,
    views::{self, project::Tab},
};
use rupicola_api::NewInvite;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Html},
        path_param,
        response::{IntoResponse, Response},
        route,
    },
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(panel(cx), account(cx), path_param::<Project>(cx).into()).await,
    )
}

async fn show(panel: Arc<Panel>, account: Account, id: String) -> PageResult<Html<String>> {
    let project = Context::load(&panel, &account, &id).await?;
    let (members, audit) = if project.role == Role::Owner {
        access_lists(&panel, &account, &id).await?
    } else {
        (Vec::new(), Vec::new())
    };
    let cx = &Cx::default();
    let data = views::project::settings::Settings {
        base: &project.base,
        role: project.role,
        members: &members,
        audit: &audit,
    };
    let body = views::project::settings::render(cx, &account, data);
    views::project::frame(cx, &account, &panel, &project, Tab::Settings, body).await
}

#[route(POST)]
async fn change_access(cx: &Cx, Form(form): Form<ProjectAction>) -> Result<Response> {
    respond(
        cx,
        submit(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[derive(Deserialize)]
struct ProjectAction {
    csrf: String,
    action: String,
    value: String,
}

/// Changes who can reach a project; returns the code of a new invitation.
async fn change_project_access(
    panel: &Panel,
    account: &Account,
    id: &str,
    action: &str,
    value: &str,
) -> PageResult<Option<String>> {
    let value = value.trim();
    let api = &panel.api;
    match action {
        "invite" => {
            let role = match value {
                "reader" => rupicola_api::Role::Reader,
                "reviewer" => rupicola_api::Role::Reviewer,
                _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
            };
            let issued = api
                .create_invite(&account.session, id, &NewInvite { role, hours: None })
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(Some(issued.code))
        }
        "revoke-member" => {
            api.remove_member(&account.session, id, value)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(None)
        }
        _ => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
}

async fn submit(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: ProjectAction,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let url = format!("/projects/{id}/settings");
    let Some(secret) =
        change_project_access(&panel, &account, &id, &form.action, &form.value).await?
    else {
        return Ok(redirect(&url));
    };
    let cx = &Cx::default();
    credential::render(cx, &account, &panel, &url, &secret)
        .await?
        .into_response(cx)
        .map_err(db_error)
}
