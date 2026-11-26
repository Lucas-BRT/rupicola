//! `GET` and `POST /projects/{project}/settings`

use super::{current_meta, tab_metadata};
use crate::app::{_workspace::title, account, fail, panel};
use crate::{
    Account, Deserialize, PageResult, Panel, StatusCode, auth,
    auth::Role,
    project::access_lists,
    safe_name,
    views::credential::credential_slip,
    views::project::{Tab, settings::settings_tab},
};
use rupicola_api::{
    Credential, DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE, DEFAULT_AUTO_RECAPTURE, DEFAULT_BASE_REF,
    DeletionPolicy, LimitValue, NewCredential, NewInvite, PolicyView, RecaptureSettings,
    RecaptureSettingsPatch, ScenarioSummary,
};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, error::see_other, page},
    view::{View, view},
};

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Settings).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let (members, audit, capture, devices, policy, limits, scenarios) =
        if project.role == Role::Owner {
            let p = panel(cx);
            let acc = account(cx);
            let (m, a) = access_lists(&p, &acc, &project.id).await.map_err(fail)?;
            let cap = crate::backend::capture_settings(cx, &project.id)
                .await
                .as_ref()
                .ok()
                .cloned()
                .unwrap_or_else(|| RecaptureSettings {
                    auto_recapture: DEFAULT_AUTO_RECAPTURE,
                    base_ref: DEFAULT_BASE_REF.to_owned(),
                    allow_agent_mismatch_override: DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE,
                });
            let devs: Vec<Credential> = p
                .api
                .list_devices(&acc.session, &project.id)
                .await
                .unwrap_or_default();
            let pol = p.api.deletion_policy(&acc.session, &project.id).await.ok();
            let lim = p.api.project_settings(&acc.session, &project.id).await.ok();
            let mut scens: Vec<(ScenarioSummary, Option<String>)> = Vec::new();
            for summary in p
                .api
                .scenarios(&acc.session, &project.id)
                .await
                .unwrap_or_default()
                .into_iter()
                .take(20)
            {
                let preview = p
                    .api
                    .scenario(&acc.session, &project.id, &summary.name)
                    .await
                    .ok()
                    .and_then(|v| serde_json::to_string_pretty(&v.scenario).ok());
                scens.push((summary, preview));
            }
            (m, a, cap, devs, pol, lim, scens)
        } else {
            (
                Vec::new(),
                Vec::new(),
                RecaptureSettings {
                    auto_recapture: DEFAULT_AUTO_RECAPTURE,
                    base_ref: DEFAULT_BASE_REF.to_owned(),
                    allow_agent_mismatch_override: DEFAULT_ALLOW_AGENT_MISMATCH_OVERRIDE,
                },
                Vec::new(),
                None,
                None,
                Vec::new(),
            )
        };
    Ok(view! {
        settings_tab(
            base: &project.base,
            role: project.role,
            members: members,
            audit: audit,
            capture: capture,
            devices: devices,
            policy: policy,
            limits: limits,
            scenarios: scenarios,
        )
    })
}

/// Changes who can reach the project; a new invitation's code is shown once.
#[page(POST, with(Metadata::new().title(title("Invitation created"))))]
async fn change_access(cx: &Cx, Form(form): Form<ProjectAction>) -> Result<impl View> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf).map_err(fail)?;
    if !safe_name(&project.id) {
        return Err(fail(StatusCode::NOT_FOUND));
    }
    if project.role != Role::Owner {
        return Err(fail(StatusCode::FORBIDDEN));
    }
    let url = format!("{}/settings", project.base);
    let secret = change_project_access(&panel, &account, &project.id, &form)
        .await
        .map_err(fail)?
        .ok_or_else(|| see_other(&url))?;
    Ok(view! { credential_slip(url: &url, secret: &secret) })
}

#[derive(Deserialize)]
struct ProjectAction {
    csrf: String,
    action: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    auto_recapture: Option<String>,
    #[serde(default)]
    allow_agent_mismatch_override: Option<String>,
    #[serde(default)]
    base_ref: Option<String>,
}

/// Changes who can reach a project, its runner devices, capture settings, deletion policy, or limits.
async fn change_project_access(
    panel: &Panel,
    account: &Account,
    id: &str,
    form: &ProjectAction,
) -> PageResult<Option<String>> {
    let value = form.value.trim();
    let api = &panel.api;
    match form.action.as_str() {
        "invite" => {
            let role = match value {
                "reader" => rupicola_api::Role::Reader,
                "reviewer" => rupicola_api::Role::Reviewer,
                _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
            };
            let issued = api
                .create_invite(&account.session, id, &NewInvite { role, hours: None })
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(Some(issued.code))
        }
        "revoke-member" => {
            api.remove_member(&account.session, id, value)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(None)
        }
        "create-device" => {
            if value.is_empty() {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            let issued = api
                .create_device(
                    &account.session,
                    id,
                    &NewCredential {
                        label: value.to_owned(),
                    },
                )
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(Some(issued.token))
        }
        "revoke-device" => {
            api.delete_device(&account.session, id, value)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(None)
        }
        "deletion-policy" | "set-limit" | "reset-limit" => {
            update_policy_or_limit(panel, account, id, form).await?;
            Ok(None)
        }
        "capture-settings" => {
            let auto_recapture = Some(form.auto_recapture.is_some());
            let allow_agent_mismatch_override = Some(form.allow_agent_mismatch_override.is_some());
            let base_ref = form
                .base_ref
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            api.update_capture_settings(
                &account.session,
                id,
                &RecaptureSettingsPatch {
                    auto_recapture,
                    base_ref,
                    allow_agent_mismatch_override,
                },
            )
            .await
            .map_err(|e| auth::api_error(&e))?;
            Ok(None)
        }
        _ => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
}

async fn update_policy_or_limit(
    panel: &Panel,
    account: &Account,
    id: &str,
    form: &ProjectAction,
) -> PageResult<()> {
    let value = form.value.trim();
    let api = &panel.api;
    match form.action.as_str() {
        "deletion-policy" => {
            let policy = match value {
                "forbid" => DeletionPolicy::Forbid,
                "confirm" => DeletionPolicy::Confirm,
                _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
            };
            api.set_deletion_policy(
                &account.session,
                id,
                &PolicyView {
                    policy,
                    server_allows: true,
                },
            )
            .await
            .map_err(|e| auth::api_error(&e))?;
        }
        "set-limit" => {
            let parsed: u64 = value
                .parse()
                .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
            api.set_project_setting(
                &account.session,
                id,
                form.key.trim(),
                &LimitValue { value: parsed },
            )
            .await
            .map_err(|e| auth::api_error(&e))?;
        }
        "reset-limit" => {
            api.reset_project_setting(&account.session, id, form.key.trim())
                .await
                .map_err(|e| auth::api_error(&e))?;
        }
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{IssuedCredential, LimitSettings, Limits};
    use rupicola_client::MockApi;
    use std::collections::BTreeMap;

    fn test_account() -> Account {
        Account {
            handle: "owner".to_owned(),
            owner: true,
            session: "sess-1".to_owned(),
            csrf: "csrf-1".to_owned(),
        }
    }

    fn action_form(action: &str, value: &str, key: &str) -> ProjectAction {
        ProjectAction {
            csrf: "csrf-1".to_owned(),
            action: action.to_owned(),
            value: value.to_owned(),
            key: key.to_owned(),
            auto_recapture: None,
            allow_agent_mismatch_override: None,
            base_ref: None,
        }
    }

    #[tokio::test]
    async fn create_and_revoke_device_and_reject_empty_label() {
        let mut api = MockApi::new();
        api.expect_create_device()
            .times(1)
            .withf(|tok, proj, form| tok == "sess-1" && proj == "demo" && form.label == "runner-1")
            .returning(|_, _, form| {
                Ok(IssuedCredential {
                    credential: Credential {
                        id: "dev-1".to_owned(),
                        kind: rupicola_api::TokenKind::Device,
                        project_id: Some("demo".to_owned()),
                        label: form.label.clone(),
                        created_at: 1,
                        last_used_at: None,
                        expires_at: None,
                    },
                    token: "rpk_dev_abc".to_owned(),
                })
            });
        api.expect_delete_device()
            .times(1)
            .withf(|tok, proj, dev| tok == "sess-1" && proj == "demo" && dev == "dev-1")
            .returning(|_, _, _| Ok(()));
        let panel = Panel::for_test(api);
        let acc = test_account();

        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("create-device", "  ", "")
            )
            .await,
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        );
        let secret = change_project_access(
            &panel,
            &acc,
            "demo",
            &action_form("create-device", "runner-1", ""),
        )
        .await
        .unwrap();
        assert_eq!(secret.as_deref(), Some("rpk_dev_abc"));

        let res = change_project_access(
            &panel,
            &acc,
            "demo",
            &action_form("revoke-device", "dev-1", ""),
        )
        .await
        .unwrap();
        assert_eq!(res, None);
    }

    #[tokio::test]
    async fn policy_and_limits_validate_inputs_and_forward_mutations() {
        let mut api = MockApi::new();
        api.expect_set_deletion_policy()
            .times(1)
            .withf(|_, proj, form| proj == "demo" && form.policy == DeletionPolicy::Forbid)
            .returning(|_, _, form| Ok(form.clone()));
        let limits = |blob| Limits {
            max_blob_bytes: blob,
            max_image_bytes: 1024,
            max_video_bytes: 4096,
            max_artifacts_per_capture: 100,
            max_body_bytes: 65536,
            max_missing_batch: 1000,
            upload_timeout_secs: 60,
            upload_idle_secs: 30,
        };
        api.expect_set_project_setting()
            .times(1)
            .withf(|_, proj, key, val| {
                proj == "demo" && key == "max_blob_bytes" && val.value == 2048
            })
            .returning(move |_, _, _, _| {
                Ok(LimitSettings {
                    effective: limits(2048),
                    set: BTreeMap::new(),
                })
            });
        api.expect_reset_project_setting()
            .times(1)
            .withf(|_, proj, key| proj == "demo" && key == "max_blob_bytes")
            .returning(move |_, _, _| {
                Ok(LimitSettings {
                    effective: limits(1024),
                    set: BTreeMap::new(),
                })
            });
        let panel = Panel::for_test(api);
        let acc = test_account();

        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("deletion-policy", "invalid", ""),
            )
            .await,
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        );
        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("deletion-policy", "forbid", ""),
            )
            .await,
            Ok(None)
        );
        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("set-limit", "not-a-number", "max_blob_bytes"),
            )
            .await,
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        );
        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("set-limit", "2048", "max_blob_bytes"),
            )
            .await,
            Ok(None)
        );
        assert_eq!(
            change_project_access(
                &panel,
                &acc,
                "demo",
                &action_form("reset-limit", "", "max_blob_bytes"),
            )
            .await,
            Ok(None)
        );
        assert_eq!(
            change_project_access(&panel, &acc, "demo", &action_form("unknown", "", "")).await,
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        );
    }
}
