//! `POST /projects/{project}/design/record`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, pages};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn record(cx: &Cx, Form(form): Form<pages::DesignForm>) -> Result<Response> {
    respond(
        cx,
        pages::record_design(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}
