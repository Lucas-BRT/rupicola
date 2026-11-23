use crate::{Account, auth::Role, project::AccessItem};
use topcoat::{
    context::Cx,
    view::{Child, view},
};

/// Who can reach the project and what happened to its access lately.
#[derive(Clone, Copy)]
pub struct Settings<'a> {
    pub base: &'a str,
    pub role: Role,
    pub members: &'a [AccessItem],
    pub audit: &'a [AccessItem],
}

pub fn render<'a>(cx: &'a Cx, account: &'a Account, data: Settings<'a>) -> Child<'a> {
    let Settings {
        base,
        role,
        members,
        audit,
    } = data;
    Child::new(view! { cx =>
            <h2>"Access & publication"</h2>
            if role != Role::Owner {
                <div class="empty"><p>"Only the workspace owner can manage project access."</p></div>
            } else {
                <div class="settings-workbench">
                    <section class="surface panel-padding">
                        <h3>"Members"</h3>
                        if members.is_empty() { <p class="muted">"No invited members."</p> }
                        for item in members {
                            <form method="post" action=(format!("{base}/settings")) class="entry">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <input type="hidden" name="action" value="revoke-member">
                                <input type="hidden" name="value" value=(&item.key)>
                                <span><strong>(&item.label)</strong><small>(&item.detail)</small></span>
                                <button class="button" type="submit">"Remove"</button>
                            </form>
                        }
                        <form method="post" action=(format!("{base}/settings")) class="stack-form">
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <input type="hidden" name="action" value="invite">
                            <label for="role">"Invitation role"</label>
                            <select name="value" id="role"><option value="reader">"Reader"</option><option value="reviewer">"Reviewer"</option></select>
                            <button class="button" type="submit">"Create invitation"</button>
                        </form>
                    </section>
                </div>
                <section class="surface panel-padding">
                    <h3>"Recent activity"</h3>
                    if audit.is_empty() { <p class="muted">"No access or publish events yet."</p> }
                    <ul class="audit">
                        for item in audit {
                            <li><span>(&item.label) " · " (&item.detail)</span><time data-epoch-ms=(&item.key)></time></li>
                        }
                    </ul>
                </section>
            }
    })
}
