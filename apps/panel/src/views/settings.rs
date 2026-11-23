use crate::{Account, PageResult, Panel, auth};
use topcoat::{context::Cx, router::content::Html, view::view};

/// The account settings page.
pub async fn render(cx: &Cx, account: &Account, panel: &Panel) -> PageResult<Html<String>> {
    let content = shell!(cx, "Settings", account, panel, "settings", {
        <div class="page-title studio-heading"><h1>"Settings"</h1></div>
        <div class="settings-workbench">
            <section class="surface panel-padding">
                <h2>"Appearance"</h2>
                <div class="stack-form">
                    <label for="appearance">"Theme on this device"</label>
                    <select id="appearance" data-theme-choice=""><option value="dark">"Dark"</option><option value="light">"Light"</option><option value="system">"System"</option></select>
                </div>
            </section>
            <section class="surface panel-padding">
                <h2>"Account"</h2><p><strong>(&account.handle)</strong></p>
                <form method="post" action="/settings">
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <button type="submit" class="button">"Sign out other sessions"</button>
                </form>
                <details class="triage-note-drawer">
                    <summary>"Change password"</summary>
                    <form class="stack-form" method="post" action="/settings/password">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="current-password">"Current password"</label>
                        <input id="current-password" name="current_password" type="password" required="" maxlength="1024" autocomplete="current-password">
                        <label for="new-password">"New password (at least 12 characters)"</label>
                        <input id="new-password" name="new_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                        <label for="confirm-password">"Confirm new password"</label>
                        <input id="confirm-password" name="confirm_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                        <button type="submit" class="button primary">"Update password"</button>
                    </form>
                </details>
            </section>
            if !account.owner {
                <section class="surface panel-padding">
                    <h2>"Join a project"</h2>
                    <form class="stack-form" method="post" action="/settings/invitation">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="invite-code">"Invitation code"</label>
                        <input id="invite-code" name="code" type="password" required="" maxlength="128">
                        <button type="submit" class="button primary">"Join project"</button>
                    </form>
                </section>
            }
            if account.owner {
                <section class="surface panel-padding">
                    <h2>"Service"</h2>
                    <p>(if panel.secure { "Cloud workspace" } else { "Local workspace" })</p>
                    <p><code>(&panel.origin)</code></p>
                </section>
            }
        </div>
    });
    auth::page(cx, content).await
}
