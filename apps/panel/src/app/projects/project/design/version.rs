//! `POST /projects/{project}/design/version`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, design_system};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn version(cx: &Cx, Form(form): Form<design_system::ActionForm>) -> Result<Response> {
    let project_id: String = path_param::<Project>(cx).into();
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
