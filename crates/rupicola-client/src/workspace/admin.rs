use super::WorkspaceClient;
use crate::{Api, Result};
use async_trait::async_trait;
use rupicola_api::{
    AuditEvent, AuthSessionOps, Credential, Credentials, IssuedCredential, IssuedInvite,
    LimitSettings, LimitValue, Member, NewCredential, NewInvite, PasswordChange, PolicyView,
    Project, ProjectAdminOps, Redeem, Register, SessionToken, Setup, TokenOps,
};

#[async_trait]
impl<A: Api> AuthSessionOps for WorkspaceClient<A> {
    async fn setup_required(&self) -> Result<bool> {
        self.api.setup_required().await
    }

    async fn setup(&self, form: &Setup) -> Result<SessionToken> {
        self.api.setup(form).await
    }

    async fn sign_in(&self, form: &Credentials) -> Result<SessionToken> {
        self.api.sign_in(form).await
    }

    async fn register(&self, form: &Register) -> Result<SessionToken> {
        self.api.register(form).await
    }

    async fn sign_out(&self) -> Result<()> {
        self.api.sign_out(&self.token).await
    }

    async fn sign_out_others(&self) -> Result<()> {
        self.api.sign_out_others(&self.token).await
    }

    async fn change_password(&self, form: &PasswordChange) -> Result<()> {
        self.api.change_password(&self.token, form).await
    }

    async fn redeem_invite(&self, form: &Redeem) -> Result<Project> {
        self.api.redeem_invite(&self.token, form).await
    }
}

#[async_trait]
impl<A: Api> TokenOps for WorkspaceClient<A> {
    async fn token_list(&self) -> Result<Vec<Credential>> {
        self.api.tokens(&self.token).await
    }

    async fn token_create(&self, form: &NewCredential) -> Result<IssuedCredential> {
        self.api.create_token(&self.token, form).await
    }

    async fn token_delete(&self, id: &str) -> Result<()> {
        self.api.delete_token(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> ProjectAdminOps for WorkspaceClient<A> {
    async fn member_list(&self, project: &str) -> Result<Vec<Member>> {
        self.api.members(&self.token, project).await
    }

    async fn member_remove(&self, project: &str, handle: &str) -> Result<()> {
        self.api.remove_member(&self.token, project, handle).await
    }

    async fn invite_create(&self, project: &str, form: &NewInvite) -> Result<IssuedInvite> {
        self.api.create_invite(&self.token, project, form).await
    }

    async fn audit_log(&self, project: &str) -> Result<Vec<AuditEvent>> {
        self.api.audit(&self.token, project).await
    }

    async fn device_list(&self, project: &str) -> Result<Vec<Credential>> {
        self.api.list_devices(&self.token, project).await
    }

    async fn device_create(&self, project: &str, form: &NewCredential) -> Result<IssuedCredential> {
        self.api.create_device(&self.token, project, form).await
    }

    async fn device_delete(&self, project: &str, device: &str) -> Result<()> {
        self.api.delete_device(&self.token, project, device).await
    }

    async fn deletion_policy_get(&self, project: &str) -> Result<PolicyView> {
        self.api.deletion_policy(&self.token, project).await
    }

    async fn deletion_policy_set(&self, project: &str, form: &PolicyView) -> Result<PolicyView> {
        self.api
            .set_deletion_policy(&self.token, project, form)
            .await
    }

    async fn project_settings_get(&self, project: &str) -> Result<LimitSettings> {
        self.api.project_settings(&self.token, project).await
    }

    async fn project_settings_set(
        &self,
        project: &str,
        key: &str,
        value: &LimitValue,
    ) -> Result<LimitSettings> {
        self.api
            .set_project_setting(&self.token, project, key, value)
            .await
    }

    async fn project_settings_reset(&self, project: &str, key: &str) -> Result<LimitSettings> {
        self.api
            .reset_project_setting(&self.token, project, key)
            .await
    }
}
