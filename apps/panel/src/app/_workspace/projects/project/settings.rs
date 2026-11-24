//! `GET` and `POST /projects/{project}/settings`

use super::{current_meta, tab_metadata};
use crate::app::{_workspace::title, account, fail, panel};
use crate::{
    Account, Deserialize, PageResult, Panel, StatusCode, auth,
    auth::Role,
    project::access_lists,
    safe_name,
    views::credential::credential_slip,
    views::project::{Tab, settings::settings_tab},
};
use rupicola_api::NewInvite;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, error::see_other, page},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Settings).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let (members, audit) = if project.role == Role::Owner {
        access_lists(&panel(cx), &account(cx), &project.id)
            .await
            .map_err(fail)?
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(view! {
        settings_tab(base: &project.base, role: project.role, members: members, audit: audit)
    })
}

/// Changes who can reach the project; a new invitation's code is shown once.
#[page(POST, with(Metadata::new().title(title("Invitation created"))))]
async fn change_access(cx: &Cx, Form(form): Form<ProjectAction>) -> Result<impl View> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf).map_err(fail)?;
    if !safe_name(&project.id) {
        return Err(fail(StatusCode::NOT_FOUND));
    }
    if project.role != Role::Owner {
        return Err(fail(StatusCode::FORBIDDEN));
    }
    let url = format!("{}/settings", project.base);
    let secret = change_project_access(&panel, &account, &project.id, &form.action, &form.value)
        .await
        .map_err(fail)?
        .ok_or_else(|| see_other(&url))?;
    Ok(view! { credential_slip(url: &url, secret: &secret) })
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
