//! Manifest schema shared by capture import and panel storage.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Project {
    pub id: String,
    pub label: String,
}

#[derive(Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub project: Project,
    pub scenario: String,
    pub revision: Option<String>,
    pub created_at_ms: i64,
    pub status: String,
    pub blocker: Option<String>,
    pub inspection: String,
    pub environment: Environment,
    pub checkpoints: Vec<Checkpoint>,
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

#[derive(Deserialize)]
pub struct Finding {
    pub note: String,
    pub evidence: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq)]
pub struct Environment {
    #[serde(default)]
    pub color_scheme: String,
    #[serde(default)]
    pub reduced_motion: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub renderer: String,
}

#[derive(Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct Clip {
    pub id: String,
    pub frames: Vec<Frame>,
    pub analysis_path: Option<String>,
    pub video_path: Option<String>,
    pub sheet_path: Option<String>,
}

#[derive(Deserialize)]
pub struct Frame {
    pub path: String,
    pub target_ms: f64,
    pub video_ms: Option<f64>,
}
