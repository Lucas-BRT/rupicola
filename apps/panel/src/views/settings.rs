use crate::components::form::{csrf, stack_form};
use crate::{Account, Panel};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// The account settings page's content.
#[component]
pub async fn settings_page(cx: &Cx) -> Result<impl View> {
    let account = context::<Account>(cx);
    let panel = context::<Arc<Panel>>(cx);
    Ok(view! {
        <div class="page-title studio-heading"><h1>"Settings"</h1></div>
        <div class="settings-workbench">
            appearance()
            account_section(account: account)
            if !account.owner {
                join_project(token: &account.csrf)
            }
            if account.owner {
                service(panel: panel)
            }
        </div>
    })
}

/// The theme choice, kept on the device.
#[component]
async fn appearance(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <section class="surface panel-padding">
            <h2>"Appearance"</h2>
            <div class="stack-form">
                <label for="appearance">"Theme on this device"</label>
                <select id="appearance" data-theme-choice=""><option value="dark">"Dark"</option><option value="light">"Light"</option><option value="system">"System"</option></select>
            </div>
        </section>
    })
}

/// Who is signed in, signing out elsewhere and the password change.
#[component]
async fn account_section(cx: &Cx, account: &Account) -> Result<impl View> {
    Ok(view! {
        <section class="surface panel-padding">
            <h2>"Account"</h2><p><strong>(&account.handle)</strong></p>
            <form method="post" action="/settings">
                csrf(token: &account.csrf)
                <button type="submit" class="button">"Sign out other sessions"</button>
            </form>
            <details class="triage-note-drawer">
                <summary>"Change password"</summary>
                stack_form(action: "/settings/password", token: &account.csrf,
                    <label for="current-password">"Current password"</label>
                    <input id="current-password" name="current_password" type="password" required="" maxlength="1024" autocomplete="current-password">
                    <label for="new-password">"New password (at least 12 characters)"</label>
                    <input id="new-password" name="new_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                    <label for="confirm-password">"Confirm new password"</label>
                    <input id="confirm-password" name="confirm_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                    <button type="submit" class="button primary">"Update password"</button>
                )
            </details>
        </section>
    })
}

/// Where a member redeems an invitation code.
#[component]
async fn join_project(cx: &Cx, token: &str) -> Result<impl View> {
    Ok(view! {
        <section class="surface panel-padding">
            <h2>"Join a project"</h2>
            stack_form(action: "/settings/invitation", token: token,
                <label for="invite-code">"Invitation code"</label>
                <input id="invite-code" name="code" type="password" required="" maxlength="128">
                <button type="submit" class="button primary">"Join project"</button>
            )
        </section>
    })
}

/// What the owner sees of the service.
#[component]
async fn service(cx: &Cx, panel: &Panel) -> Result<impl View> {
    Ok(view! {
        <section class="surface panel-padding">
            <h2>"Service"</h2>
            <p>(if panel.secure { "Cloud workspace" } else { "Local workspace" })</p>
            <p><code>(&panel.origin)</code></p>
        </section>
    })
}
