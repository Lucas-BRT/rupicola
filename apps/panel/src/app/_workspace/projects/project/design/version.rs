//! `POST /projects/{project}/design/version`

use crate::app::{account, panel, respond};
use crate::{app::_workspace::projects::project::project as project_param, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

#[route(POST)]
async fn version(cx: &Cx, Form(form): Form<design_system::ActionForm>) -> Result<Response> {
    let project_id: String = project_param(cx).into();
    respond(
        cx,
        Box::pin(design_system::action(
            panel(cx),
            account(cx),
            &project_id,
            &form,
        ))
        .await,
    )
}
