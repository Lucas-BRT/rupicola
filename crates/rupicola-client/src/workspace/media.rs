use super::WorkspaceClient;
use crate::{Api, Result};
use async_trait::async_trait;
use rupicola_api::{
    ArtifactChunk, Capture, CaptureExecutionOps, MediaInspectOps, VisualDiffOps,
};
use serde_json::json;

#[async_trait]
impl<A: Api> CaptureExecutionOps for WorkspaceClient<A> {
    async fn capture_run_scenario(&self, req: &rupicola_api::CaptureRunRequest) -> Result<Capture> {
        self.call_mcp("capture_run_scenario", json!(req)).await
    }

    async fn capture_record_clip(&self, req: &rupicola_api::RecordClipRequest) -> Result<Capture> {
        self.call_mcp("capture_record_clip", json!(req)).await
    }

    async fn capture_snapshot_checkpoint(
        &self,
        req: &rupicola_api::SnapshotCheckpointRequest,
    ) -> Result<Capture> {
        self.call_mcp("capture_snapshot_checkpoint", json!(req))
            .await
    }
}

#[async_trait]
impl<A: Api> MediaInspectOps for WorkspaceClient<A> {
    async fn media_probe(&self, artifact_id: &str) -> Result<rupicola_api::MediaProbe> {
        self.call_mcp("media_probe", json!({"artifact_id": artifact_id}))
            .await
    }

    async fn media_motion_diff(
        &self,
        artifact_id: &str,
        threshold: f64,
    ) -> Result<rupicola_api::MotionDiffReport> {
        self.call_mcp(
            "media_motion_diff",
            json!({"artifact_id": artifact_id, "threshold": threshold}),
        )
        .await
    }

    async fn media_contact_sheet(
        &self,
        artifact_id: &str,
        req: &rupicola_api::ContactSheetRequest,
    ) -> Result<ArtifactChunk> {
        let mut args = json!(req);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("media_contact_sheet", args).await
    }

    async fn media_sample_frames(
        &self,
        artifact_id: &str,
        strategy: &rupicola_api::FrameSampleStrategy,
    ) -> Result<rupicola_api::SampledFrames> {
        let mut args = json!(strategy);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("media_sample_frames", args).await
    }
}

#[async_trait]
impl<A: Api> VisualDiffOps for WorkspaceClient<A> {
    async fn visual_diff_images(
        &self,
        before_artifact_id: &str,
        after_artifact_id: &str,
        mode: rupicola_api::VisualDiffMode,
    ) -> Result<rupicola_api::VisualDiffReport> {
        self.call_mcp(
            "visual_diff_images",
            json!({
                "before_artifact_id": before_artifact_id,
                "after_artifact_id": after_artifact_id,
                "mode": mode
            }),
        )
        .await
    }

    async fn visual_crop_region(
        &self,
        artifact_id: &str,
        rect: &rupicola_api::CropRect,
    ) -> Result<ArtifactChunk> {
        let mut args = json!(rect);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("visual_crop_region", args).await
    }
}
