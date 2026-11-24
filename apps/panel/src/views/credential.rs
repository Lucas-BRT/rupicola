use crate::Panel;
use crate::components::{display::link_button, layout::breadcrumbs};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// The content that shows a new invitation's code once.
#[component]
pub async fn credential_slip(cx: &Cx, url: &str, secret: &str) -> Result<impl View> {
    let panel = context::<Arc<Panel>>(cx);
    Ok(view! {
        breadcrumbs(<a href=(url)>"Project settings"</a>)
        <h1>"Invitation created"</h1>
        <section class="surface panel-padding credential-slip">
            <p>"Share the code and the registration URL with the invitee. The code expires in 24 hours."</p>
            <p><code>(format!("{}/register", panel.origin))</code></p>
            <label for="secret">"Copy this code now"</label>
            <input id="secret" class="secret" readonly="" value=(secret)>
            <p>link_button(href: url, "Return to settings")</p>
        </section>
    })
}
