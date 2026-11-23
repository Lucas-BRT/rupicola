//! `POST /projects/{project}/design/edit`: replace the document of a draft.

use crate::app::{account, panel, respond};
use crate::{
    Deserialize, PageResult, StatusCode,
    app::_workspace::projects::project::project as project_param,
    auth::{self, Role},
    redirect, safe_name,
};
use rupicola_api::{Document, DocumentEdit, Guidance, HeadingStyle};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[derive(Deserialize)]
struct EditForm {
    csrf: String,
    version: String,
    generation: i64,
    name: String,
    brand: String,
    landing_title: String,
    queue_title: String,
    queue_item: String,
    settings_title: String,
    direction: String,
    usage: String,
    screen_title: String,
    scenario: String,
    before_label: String,
    after_label: String,
    canvas: String,
    surface: String,
    ink: String,
    accent: String,
    action_ink: String,
    success: String,
    error: String,
    spacing: u8,
    radius: u8,
    heading: HeadingStyle,
    button_notes: String,
    form_notes: String,
    table_notes: String,
    comparison_notes: String,
}

#[route(POST)]
async fn edit(cx: &Cx, Form(form): Form<EditForm>) -> Result<Response> {
    respond(cx, submit(cx, project_param(cx).into(), form).await)
}

async fn submit(cx: &Cx, project: String, form: EditForm) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&project) {
        return Err(StatusCode::NOT_FOUND);
    }
    if auth::role_of(cx, &project).await? != Role::Owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let document = Document {
        name: form.name,
        brand: form.brand,
        landing_title: form.landing_title,
        queue_title: form.queue_title,
        queue_item: form.queue_item,
        settings_title: form.settings_title,
        direction: form.direction,
        usage: form.usage,
        screen_title: form.screen_title,
        scenario: form.scenario,
        before_label: form.before_label,
        after_label: form.after_label,
        canvas: form.canvas,
        surface: form.surface,
        ink: form.ink,
        accent: form.accent,
        action_ink: form.action_ink,
        success: form.success,
        error: form.error,
        spacing: form.spacing,
        radius: form.radius,
        heading: form.heading,
        guidance: Guidance {
            button: form.button_notes,
            form: form.form_notes,
            table: form.table_notes,
            comparison: form.comparison_notes,
        },
    };
    let saved = panel
        .api
        .edit_design_system(
            &account.session,
            &project,
            &form.version,
            &DocumentEdit {
                generation: form.generation,
                document,
            },
        )
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!(
        "/projects/{project}/design?view=system&version={}",
        saved.id
    )))
}
