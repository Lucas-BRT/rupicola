use crate::components::{
    controls::{SelectOption, select_menu},
    display::empty,
    form::csrf,
};
use crate::{Account, auth::Role, project::AccessItem};
use rupicola_api::{Credential, DeletionPolicy, LimitSettings, PolicyView, RecaptureSettings};
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// The settings tab's content: members, runner devices, capture settings, deletion policy, limits, and activity.
#[component]
#[allow(clippy::too_many_arguments)]
pub async fn settings_tab(
    cx: &Cx,
    base: &str,
    role: Role,
    members: Vec<AccessItem>,
    audit: Vec<AccessItem>,
    capture: RecaptureSettings,
    devices: Vec<Credential>,
    policy: Option<PolicyView>,
    limits: Option<LimitSettings>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    let role_options = [
        SelectOption::new("reader", "Reader"),
        SelectOption::new("reviewer", "Reviewer"),
    ];
    let action_url = format!("{base}/settings");
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
                        member_row(action: action_url.clone(), token: &account.csrf, item: item)
                    }
                    <form method="post" action=(&action_url) class="stack-form">
                        csrf(token: &account.csrf)
                        <input type="hidden" name="action" value="invite">
                        <label for="role-trigger">"Invitation role"</label>
                        select_menu(
                            id: "role",
                            name: "value",
                            label: "Invitation role",
                            selected: "reader",
                            options: &role_options,
                        )
                        <button class="button primary" type="submit">"Create invitation"</button>
                    </form>
                </section>
                devices_card(action: &action_url, token: &account.csrf, devices: &devices)
                <section class="surface panel-padding">
                    <h3>"Capture & recapture"</h3>
                    <form method="post" action=(&action_url) class="stack-form">
                        csrf(token: &account.csrf)
                        <input type="hidden" name="action" value="capture-settings">
                        <label class="checkbox-row" for="auto-recapture">
                            <input type="checkbox" id="auto-recapture" name="auto_recapture" value="true" if capture.auto_recapture { checked="" }>
                            <span>"Automatically recapture pending fixes when a fix is approved"</span>
                        </label>
                        <label for="base-ref">"Base Git ref"</label>
                        <input type="text" id="base-ref" name="base_ref" value=(&capture.base_ref) maxlength="200" required="">
                        <button class="button primary" type="submit">"Save capture settings"</button>
                    </form>
                </section>
                policy_and_limits_card(
                    action: &action_url,
                    token: &account.csrf,
                    policy: policy.as_ref(),
                    limits: limits.as_ref(),
                )
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

#[component]
async fn devices_card(
    cx: &Cx,
    action: &str,
    token: &str,
    devices: &[Credential],
) -> Result<impl View> {
    Ok(view! {
        <section class="surface panel-padding">
            <h3>"Capture devices (runners)"</h3>
            if devices.is_empty() { <p class="muted">"No capture devices provisioned yet."</p> }
            for dev in devices {
                <form method="post" action=(action) class="entry">
                    csrf(token: token)
                    <input type="hidden" name="action" value="revoke-device">
                    <input type="hidden" name="value" value=(&dev.id)>
                    <span><strong>(&dev.label)</strong><small class="mono">(&dev.id)</small></span>
                    <button class="button button-danger" type="submit">"Revoke"</button>
                </form>
            }
            <form method="post" action=(action) class="stack-form">
                csrf(token: token)
                <input type="hidden" name="action" value="create-device">
                <label for="device-label">"Device label"</label>
                <input type="text" id="device-label" name="value" placeholder="ci-linux-runner" maxlength="80" required="">
                <button class="button primary" type="submit">"Issue device token"</button>
            </form>
        </section>
    })
}

#[component]
async fn policy_and_limits_card(
    cx: &Cx,
    action: &str,
    token: &str,
    policy: Option<&PolicyView>,
    limits: Option<&LimitSettings>,
) -> Result<impl View> {
    let policy_options = [
        SelectOption::new("confirm", "Allow with confirmation"),
        SelectOption::new("forbid", "Forbid deleting cited captures"),
    ];
    let limit_key_options = [
        SelectOption::new("max_blob_bytes", "Max blob bytes"),
        SelectOption::new("max_image_bytes", "Max image bytes"),
        SelectOption::new("max_video_bytes", "Max video bytes"),
        SelectOption::new("max_artifacts_per_capture", "Max artifacts per capture"),
        SelectOption::new("upload_timeout_secs", "Upload timeout (s)"),
        SelectOption::new("upload_idle_secs", "Upload idle timeout (s)"),
    ];
    let selected_policy = match policy.map(|p| p.policy) {
        Some(DeletionPolicy::Forbid) => "forbid",
        _ => "confirm",
    };
    let server_allows = policy.is_none_or(|p| p.server_allows);
    Ok(view! {
        <section class="surface panel-padding">
            <h3>"Deletion policy & upload limits"</h3>
            if !server_allows {
                <p class="notice">"Server policy currently forbids deleting cited captures."</p>
            }
            <form method="post" action=(action) class="stack-form">
                csrf(token: token)
                <input type="hidden" name="action" value="deletion-policy">
                <label for="del-policy-trigger">"Cited capture deletion"</label>
                select_menu(
                    id: "del-policy",
                    name: "value",
                    label: "Cited capture deletion",
                    selected: selected_policy,
                    options: &policy_options,
                )
                <button class="button primary" type="submit">"Save deletion policy"</button>
            </form>
            if let Some(lim) = limits {
                <dl class="facts">
                    <dt>"Max blob"</dt><dd class="mono">(format!("{} B", lim.effective.max_blob_bytes))</dd>
                    <dt>"Max image"</dt><dd class="mono">(format!("{} B", lim.effective.max_image_bytes))</dd>
                    <dt>"Max video"</dt><dd class="mono">(format!("{} B", lim.effective.max_video_bytes))</dd>
                    <dt>"Max artifacts"</dt><dd class="mono">(lim.effective.max_artifacts_per_capture.to_string())</dd>
                </dl>
                for (k, v) in &lim.set {
                    <form method="post" action=(action) class="entry">
                        csrf(token: token)
                        <input type="hidden" name="action" value="reset-limit">
                        <input type="hidden" name="key" value=(k.as_str())>
                        <span><strong class="mono">(k.as_str())</strong><small class="mono">(v.to_string())</small></span>
                        <button class="button" type="submit">"Reset"</button>
                    </form>
                }
                <form method="post" action=(action) class="stack-form">
                    csrf(token: token)
                    <input type="hidden" name="action" value="set-limit">
                    <label for="limit-key-trigger">"Override limit"</label>
                    select_menu(
                        id: "limit-key",
                        name: "key",
                        label: "Override limit",
                        selected: "max_blob_bytes",
                        options: &limit_key_options,
                    )
                    <label for="limit-val">"Value"</label>
                    <input type="number" id="limit-val" name="value" min="1" required="">
                    <button class="button primary" type="submit">"Save limit"</button>
                </form>
            }
        </section>
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
            <button class="button button-danger" type="submit">"Remove"</button>
        </form>
    })
}
