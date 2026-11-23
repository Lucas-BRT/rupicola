use crate::{Account, PageResult, Panel, auth};
use topcoat::{context::Cx, router::content::Html, view::view};

/// The page that shows a new invitation's code once.
pub async fn render(
    cx: &Cx,
    account: &Account,
    panel: &Panel,
    url: &str,
    secret: &str,
) -> PageResult<Html<String>> {
    let (url, secret) = (url, secret);
    let content = shell!(cx, "Copy credential", account, panel, "projects", {
        <nav class="breadcrumbs" aria-label="Breadcrumb"><a href=(url)>"Project settings"</a></nav>
        <h1>"Invitation created"</h1>
        <section class="surface panel-padding credential-slip">
            <p>"Share the code and the registration URL with the invitee. The code expires in 24 hours."</p>
            <p><code>(format!("{}/register", panel.origin))</code></p>
            <label for="secret">"Copy this code now"</label>
            <input id="secret" class="secret" readonly="" value=(secret)>
            <p><a class="button" href=(url)>"Return to settings"</a></p>
        </section>
    });
    auth::page(cx, content).await
}
