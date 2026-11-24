use crate::components::{display::empty, form::csrf};
use crate::{Account, auth::Role, project::AccessItem};
use rupicola_api::RecaptureSettings;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// The settings tab's content: who can reach the project and what happened to its access lately.
#[component]
pub async fn settings_tab(
    cx: &Cx,
    base: &str,
    role: Role,
    members: Vec<AccessItem>,
    audit: Vec<AccessItem>,
    capture: RecaptureSettings,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    Ok(view! {
        <h2>"Access & publication"</h2>
        if role != Role::Owner {
            empty(message: "Only the workspace owner can manage project access.")
        } else {
            <div class="settings-workbench">
                <section class="surface panel-padding">
                    <h3>"Members"</h3>
                    if members.is_empty() { <p class="muted">"No invited members."</p> }
                    for item in &members {
                        member_row(action: format!("{base}/settings"), token: &account.csrf, item: item)
                    }
                    <form method="post" action=(format!("{base}/settings")) class="stack-form">
                        csrf(token: &account.csrf)
                        <input type="hidden" name="action" value="invite">
                        <label for="role">"Invitation role"</label>
                        <select name="value" id="role"><option value="reader">"Reader"</option><option value="reviewer">"Reviewer"</option></select>
                        <button class="button" type="submit">"Create invitation"</button>
                    </form>
                </section>
                <section class="surface panel-padding">
                    <h3>"Capture & recapture"</h3>
                    <form method="post" action=(format!("{base}/settings")) class="stack-form">
                        csrf(token: &account.csrf)
                        <input type="hidden" name="action" value="capture-settings">
                        <label class="checkbox-row" for="auto-recapture">
                            <input type="checkbox" id="auto-recapture" name="auto_recapture" value="true" if capture.auto_recapture { checked="" }>
                            <span>"Automatically recapture pending fixes when a fix is approved"</span>
                        </label>
                        <label for="base-ref">"Base Git ref"</label>
                        <input type="text" id="base-ref" name="base_ref" value=(&capture.base_ref) maxlength="200" required="">
                        <button class="button" type="submit">"Save capture settings"</button>
                    </form>
                </section>
            </div>
            <section class="surface panel-padding">
                <h3>"Recent activity"</h3>
                if audit.is_empty() { <p class="muted">"No access or publish events yet."</p> }
                <ul class="audit">
                    for item in &audit {
                        <li><span>(&item.label) " · " (&item.detail)</span><time data-epoch-ms=(&item.key)></time></li>
                    }
                </ul>
            </section>
        }
    })
}

/// A member of the project, with the button that removes them.
#[component]
async fn member_row(cx: &Cx, action: String, token: &str, item: &AccessItem) -> Result<impl View> {
    Ok(view! {
        <form method="post" action=(action) class="entry">
            csrf(token: token)
            <input type="hidden" name="action" value="revoke-member">
            <input type="hidden" name="value" value=(&item.key)>
            <span><strong>(&item.label)</strong><small>(&item.detail)</small></span>
            <button class="button" type="submit">"Remove"</button>
        </form>
    })
}
