use super::ServerOps;
use crate::{
    domain::ApiError,
    services::{capture_exec, media, visual},
};
use async_trait::async_trait;
use rupicola_api::{ArtifactChunk, Capture, CaptureExecutionOps, MediaInspectOps, VisualDiffOps};

#[async_trait]
impl CaptureExecutionOps for ServerOps {
    async fn capture_run_scenario(
        &self,
        req: &rupicola_api::CaptureRunRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::run_scenario(&self.state, &self.principal, req).await
    }

    async fn capture_record_clip(
        &self,
        req: &rupicola_api::RecordClipRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::record_clip(&self.state, &self.principal, req).await
    }

    async fn capture_snapshot_checkpoint(
        &self,
        req: &rupicola_api::SnapshotCheckpointRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::snapshot_checkpoint(&self.state, &self.principal, req).await
    }
}

#[async_trait]
impl MediaInspectOps for ServerOps {
    async fn media_probe(&self, artifact_id: &str) -> Result<rupicola_api::MediaProbe, ApiError> {
        media::probe(&self.state, &self.principal, artifact_id).await
    }

    async fn media_motion_diff(
        &self,
        artifact_id: &str,
        threshold: f64,
    ) -> Result<rupicola_api::MotionDiffReport, ApiError> {
        media::motion_diff(&self.state, &self.principal, artifact_id, threshold).await
    }

    async fn media_contact_sheet(
        &self,
        artifact_id: &str,
        req: &rupicola_api::ContactSheetRequest,
    ) -> Result<ArtifactChunk, ApiError> {
        media::contact_sheet(&self.state, &self.principal, artifact_id, req).await
    }

    async fn media_sample_frames(
        &self,
        artifact_id: &str,
        strategy: &rupicola_api::FrameSampleStrategy,
    ) -> Result<rupicola_api::SampledFrames, ApiError> {
        media::sample_frames(&self.state, &self.principal, artifact_id, strategy).await
    }
}

#[async_trait]
impl VisualDiffOps for ServerOps {
    async fn visual_diff_images(
        &self,
        before_artifact_id: &str,
        after_artifact_id: &str,
        mode: rupicola_api::VisualDiffMode,
    ) -> Result<rupicola_api::VisualDiffReport, ApiError> {
        visual::diff_images(
            &self.state,
            &self.principal,
            before_artifact_id,
            after_artifact_id,
            mode,
        )
        .await
    }

    async fn visual_crop_region(
        &self,
        artifact_id: &str,
        rect: &rupicola_api::CropRect,
    ) -> Result<ArtifactChunk, ApiError> {
        visual::crop_region(&self.state, &self.principal, artifact_id, rect).await
    }
}
