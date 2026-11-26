//! Composed supertraits built from atomic capability traits via blanket impls.
//!
//! Implementors only implement the atomic traits they support; any consumer can
//! bound on the narrowest supertrait or atomic trait for its scope.

use super::{
    ArtifactReadOps, AuthSessionOps, BlobTransferOps, CaptureCatalogOps, CaptureExecutionOps,
    CaptureLifecycleOps, CaptureLineageOps, CapturePublishOps, DesignLogOps, DesignSystemReadOps,
    DesignSystemWriteOps, FixReadOps, FixSelectionOps, FixWriteOps, IdentityOps, JobQueueOps,
    JobWorkerOps, MediaInspectOps, OfflineCatalogOps, OfflineScanOps, ProblemReadOps,
    ProblemSelectionOps, ProblemWriteOps, ProjectAdminOps, ProjectCrudOps, ProjectPolicyOps,
    ScenarioOps, SelectionOps, TokenOps, TopicReadOps, TopicWriteOps, VisualDiffOps,
};

// ── Domain-level supertraits ──────────────────────────────────────────────────

pub trait CaptureOps:
    CaptureCatalogOps + CapturePublishOps + CaptureLifecycleOps + CaptureLineageOps + ArtifactReadOps
{
}
impl<T> CaptureOps for T where
    T: CaptureCatalogOps
        + CapturePublishOps
        + CaptureLifecycleOps
        + CaptureLineageOps
        + ArtifactReadOps
{
}

pub trait MediaAnalysisOps: MediaInspectOps + VisualDiffOps {}
impl<T> MediaAnalysisOps for T where T: MediaInspectOps + VisualDiffOps {}

pub trait ProblemOps: ProblemReadOps + ProblemWriteOps + ProblemSelectionOps {}
impl<T> ProblemOps for T where T: ProblemReadOps + ProblemWriteOps + ProblemSelectionOps {}

pub trait FixOps: FixReadOps + FixWriteOps + FixSelectionOps {}
impl<T> FixOps for T where T: FixReadOps + FixWriteOps + FixSelectionOps {}

pub trait DesignOps: DesignLogOps + DesignSystemReadOps + DesignSystemWriteOps {}
impl<T> DesignOps for T where T: DesignLogOps + DesignSystemReadOps + DesignSystemWriteOps {}

pub trait TopicOps: TopicReadOps + TopicWriteOps {}
impl<T> TopicOps for T where T: TopicReadOps + TopicWriteOps {}

pub trait JobOps: JobQueueOps + JobWorkerOps {}
impl<T> JobOps for T where T: JobQueueOps + JobWorkerOps {}

pub trait OfflineOps: OfflineScanOps + OfflineCatalogOps {}
impl<T> OfflineOps for T where T: OfflineScanOps + OfflineCatalogOps {}

// ── Scope-level supertraits ───────────────────────────────────────────────────

/// Read-only visual & static auditor scope (cannot mutate backend state).
pub trait AuditorOps:
    IdentityOps
    + CaptureCatalogOps
    + ArtifactReadOps
    + MediaAnalysisOps
    + ProblemReadOps
    + FixReadOps
    + DesignSystemReadOps
    + OfflineOps
{
}
impl<T> AuditorOps for T where
    T: IdentityOps
        + CaptureCatalogOps
        + ArtifactReadOps
        + MediaAnalysisOps
        + ProblemReadOps
        + FixReadOps
        + DesignSystemReadOps
        + OfflineOps
{
}

/// Canonical active workspace contract shared by MCP, `ServerOps`, and `WorkspaceClient`.
pub trait WorkspaceOps:
    IdentityOps
    + ProjectCrudOps
    + ProjectPolicyOps
    + CaptureOps
    + CaptureExecutionOps
    + MediaAnalysisOps
    + ProblemOps
    + FixOps
    + DesignOps
    + JobOps
    + ScenarioOps
    + SelectionOps
    + TopicOps
    + OfflineOps
{
}
impl<T> WorkspaceOps for T where
    T: IdentityOps
        + ProjectCrudOps
        + ProjectPolicyOps
        + CaptureOps
        + CaptureExecutionOps
        + MediaAnalysisOps
        + ProblemOps
        + FixOps
        + DesignOps
        + JobOps
        + ScenarioOps
        + SelectionOps
        + TopicOps
        + OfflineOps
{
}

/// Full autonomous agent target contract.
pub trait FullAgentOps: WorkspaceOps {}
impl<T> FullAgentOps for T where T: WorkspaceOps {}

/// Minimal scope required by a capture job runner (`apps/photographer`).
pub trait RunnerOps: ScenarioOps + JobWorkerOps + BlobTransferOps {}
impl<T> RunnerOps for T where T: ScenarioOps + JobWorkerOps + BlobTransferOps {}

/// Account authentication, token, and project-admin scope.
pub trait AdminAuthOps: AuthSessionOps + TokenOps + ProjectAdminOps {}
impl<T> AdminAuthOps for T where T: AuthSessionOps + TokenOps + ProjectAdminOps {}

/// Scope for CLI commands (workspace + login/session + binary bundle publishing).
pub trait CliOps: WorkspaceOps + AuthSessionOps + BlobTransferOps {}
impl<T> CliOps for T where T: WorkspaceOps + AuthSessionOps + BlobTransferOps {}

/// Scope for the web panel (`apps/panel`).
pub trait PanelOps: WorkspaceOps + AdminAuthOps + BlobTransferOps {}
impl<T> PanelOps for T where T: WorkspaceOps + AdminAuthOps + BlobTransferOps {}
