//! `POST /projects/{project}/design/edit`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn edit(cx: &Cx, Form(form): Form<design_system::EditForm>) -> Result<Response> {
    let project_id: String = path_param::<Project>(cx).into();
    respond(
        cx,
        design_system::edit(&panel(cx), &account(cx), &project_id, form).await,
    )
}
