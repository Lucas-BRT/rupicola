//! `POST /projects/{project}/design/record`

use crate::app::projects::project::Project;
use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, Deserialize, PageResult, Panel, StatusCode, auth, auth::Role, redirect, safe_name,
};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn record(cx: &Cx, Form(form): Form<DesignForm>) -> Result<Response> {
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
struct DesignForm {
    #[serde(default)]
    csrf: String,
    kind: rupicola_api::DesignKind,
    status: rupicola_api::DesignStatus,
    title: String,
    note: String,
    surface: Option<String>,
    reference: Option<String>,
    evidence: Option<String>,
    supersedes: Option<String>,
}

async fn submit(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: DesignForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if auth::role(&panel, &account, &id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    save_design(&panel, &account, &id, form).await?;
    Ok(redirect(&format!("/projects/{id}/design")))
}

async fn save_design(
    panel: &Panel,
    account: &Account,
    id: &str,
    form: DesignForm,
) -> PageResult<String> {
    let clean =
        |value: Option<String>| value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let reference = clean(form.reference);
    let title = form.title.trim();
    let note = form.note.trim();
    if title.is_empty()
        || note.is_empty()
        || reference.as_ref().is_some_and(|x| {
            !url::Url::parse(x)
                .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
        })
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let new = rupicola_api::NewDesignEntry {
        kind: form.kind,
        status: form.status,
        title: title.into(),
        note: note.into(),
        surface: clean(form.surface),
        supersedes: clean(form.supersedes),
        reference,
        evidence: clean(form.evidence),
    };
    panel
        .api
        .add_design(&account.session, id, &new)
        .await
        .map(|entry| entry.id)
        .map_err(|e| auth::api_error(&e))
}
