//! Binary blob transfer, authentication/session, token, and project-admin traits.

use super::{ArtifactBytes, OpsBase};
use crate::{
    AuditEvent, Capture, Credential, Credentials, IssuedCredential, IssuedInvite, LimitSettings,
    LimitValue, Member, Missing, MissingQuery, NewCredential, NewInvite, PasswordChange,
    PolicyView, Project, Redeem, Register, SessionToken, Setup, record::CaptureRecord,
};
use async_trait::async_trait;

/// Binary blob deduplication, upload, record publication, and byte-range downloads.
#[async_trait]
pub trait BlobTransferOps: OpsBase {
    async fn missing_blobs(
        &self,
        project: &str,
        query: &MissingQuery,
    ) -> Result<Missing, Self::Error>;

    async fn upload_project_blob(
        &self,
        project: &str,
        sha256: &str,
        data: Vec<u8>,
    ) -> Result<(), Self::Error>;

    async fn publish_record(
        &self,
        project: &str,
        id: &str,
        record: &CaptureRecord,
    ) -> Result<Capture, Self::Error>;

    async fn artifact_bytes(
        &self,
        id: &str,
        range: Option<String>,
    ) -> Result<ArtifactBytes, Self::Error>;
}

/// Interactive account setup, sign-in, password, and session management.
#[async_trait]
pub trait AuthSessionOps: OpsBase {
    async fn setup_required(&self) -> Result<bool, Self::Error>;
    async fn setup(&self, form: &Setup) -> Result<SessionToken, Self::Error>;
    async fn sign_in(&self, form: &Credentials) -> Result<SessionToken, Self::Error>;
    async fn register(&self, form: &Register) -> Result<SessionToken, Self::Error>;
    async fn sign_out(&self) -> Result<(), Self::Error>;
    async fn sign_out_others(&self) -> Result<(), Self::Error>;
    async fn change_password(&self, form: &PasswordChange) -> Result<(), Self::Error>;
    async fn redeem_invite(&self, form: &Redeem) -> Result<Project, Self::Error>;
}

/// Personal session and machine-token management.
#[async_trait]
pub trait TokenOps: OpsBase {
    async fn token_list(&self) -> Result<Vec<Credential>, Self::Error>;
    async fn token_create(&self, form: &NewCredential) -> Result<IssuedCredential, Self::Error>;
    async fn token_delete(&self, id: &str) -> Result<(), Self::Error>;
}

/// Project membership, devices, invitations, limits, deletion policy, and audit trail administration.
#[async_trait]
pub trait ProjectAdminOps: OpsBase {
    async fn member_list(&self, project: &str) -> Result<Vec<Member>, Self::Error>;
    async fn member_remove(&self, project: &str, handle: &str) -> Result<(), Self::Error>;
    async fn invite_create(
        &self,
        project: &str,
        form: &NewInvite,
    ) -> Result<IssuedInvite, Self::Error>;
    async fn audit_log(&self, project: &str) -> Result<Vec<AuditEvent>, Self::Error>;
    async fn device_list(&self, project: &str) -> Result<Vec<Credential>, Self::Error>;
    async fn device_create(
        &self,
        project: &str,
        form: &NewCredential,
    ) -> Result<IssuedCredential, Self::Error>;
    async fn device_delete(&self, project: &str, device: &str) -> Result<(), Self::Error>;
    async fn deletion_policy_get(&self, project: &str) -> Result<PolicyView, Self::Error>;
    async fn deletion_policy_set(
        &self,
        project: &str,
        form: &PolicyView,
    ) -> Result<PolicyView, Self::Error>;
    async fn project_settings_get(&self, project: &str) -> Result<LimitSettings, Self::Error>;
    async fn project_settings_set(
        &self,
        project: &str,
        key: &str,
        value: &LimitValue,
    ) -> Result<LimitSettings, Self::Error>;
    async fn project_settings_reset(
        &self,
        project: &str,
        key: &str,
    ) -> Result<LimitSettings, Self::Error>;
}
