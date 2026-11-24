//! `POST /projects/{project}/design/version`: create a design-system version or act on one.

use crate::app::{account, panel, respond};
use crate::{
    Deserialize, PageResult, StatusCode,
    app::_workspace::projects::project::project as project_param,
    auth::{self, Role},
    redirect, safe_name,
};
use rupicola_api::{SystemAction, SystemActionForm};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[derive(Deserialize)]
struct ActionForm {
    #[serde(default)]
    csrf: String,
    action: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    generation: i64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    target: String,
}

#[route(POST)]
async fn version(cx: &Cx, Form(form): Form<ActionForm>) -> Result<Response> {
    respond(cx, submit(cx, project_param(cx).into(), form).await)
}

/// The design page showing `id` under `view`.
fn location(project: &str, view: &str, id: &str) -> Response {
    redirect(&format!(
        "/projects/{project}/design?view={view}&version={id}"
    ))
}

async fn submit(cx: &Cx, project: String, form: ActionForm) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&project) {
        return Err(StatusCode::NOT_FOUND);
    }
    if auth::role_of(cx, &project).await? != Role::Owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let session = &account.session;
    if form.action == "create" {
        let draft = panel
            .api
            .create_design_system(session, &project)
            .await
            .map_err(|e| auth::api_error(&e))?;
        return Ok(location(&project, "system", &draft.id));
    }
    let action: SystemAction = form
        .action
        .parse()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let done = panel
        .api
        .act_on_design_system(
            session,
            &project,
            &form.version,
            &SystemActionForm {
                action,
                generation: form.generation,
                target: form.target,
                note: form.note,
            },
        )
        .await
        .map_err(|e| auth::api_error(&e))?;
    let view = match action {
        SystemAction::Validate => "screens",
        SystemAction::Submit
        | SystemAction::Comment
        | SystemAction::Approve
        | SystemAction::Changes => "proposals",
        SystemAction::Adopt | SystemAction::Variation => "system",
    };
    Ok(location(&project, view, &done.id))
}
