//! `POST /projects/{project}/design/edit`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::project as project_param, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn edit(cx: &Cx, Form(form): Form<design_system::EditForm>) -> Result<Response> {
    let project_id: String = project_param(cx).into();
    respond(
        cx,
        design_system::edit(&panel(cx), &account(cx), &project_id, form).await,
    )
}
