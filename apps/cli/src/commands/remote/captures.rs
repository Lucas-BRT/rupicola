//! The captures of a project and what reviewers noted on them.

use super::client::{Client, Scoped, rows};
use crate::{Result, block_on};
use rupicola_api::{Environment, NewFinding, RunCapture};
use rupicola_client::Api;

#[derive(clap::Args)]
pub struct CapturesArgs {
    #[command(subcommand)]
    action: CapturesAction,
}

#[derive(clap::Subcommand)]
enum CapturesAction {
    /// List a project's captures, newest first.
    List(Scoped),
    /// Show one capture with its checkpoints, clips and findings.
    Show {
        id: String,
        #[command(flatten)]
        client: Client,
    },
    /// Note what you saw on a capture; the capture becomes reviewed.
    Note {
        id: String,
        /// What you saw.
        text: String,
        /// Artifact that shows it.
        #[arg(long)]
        evidence: Option<String>,
        #[command(flatten)]
        client: Client,
    },
    /// Ask the server to capture a live HTTP(S) URL into the project.
    Run {
        url: String,
        #[arg(long, default_value = "Web capture")]
        scenario: String,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        #[arg(long)]
        color_scheme: Option<String>,
        #[arg(long)]
        reduced_motion: bool,
        #[command(flatten)]
        scope: Scoped,
    },
}

impl CapturesArgs {
    pub(crate) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action {
                CapturesAction::List(scope) => {
                    let remote = scope.client().remote()?;
                    let captures = remote
                        .api
                        .captures(&remote.token, &scope.project()?)
                        .await?;
                    scope.client().show(&captures, |captures| {
                        rows(captures, "No captures.", |capture| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                capture.id, capture.status, capture.inspection, capture.scenario
                            )
                        });
                    })
                }
                CapturesAction::Show { id, client } => {
                    let remote = client.remote()?;
                    let capture = remote.api.capture(&remote.token, &id).await?;
                    client.show(&capture, |capture| {
                        println!(
                            "{} {} ({}, {})",
                            capture.id, capture.scenario, capture.status, capture.inspection
                        );
                        if let Some(blocker) = &capture.blocker {
                            println!("blocked: {blocker}");
                        }
                        println!(
                            "{} checkpoints, {} clips, {} findings",
                            capture.checkpoints.len(),
                            capture.clips.len(),
                            capture.findings.len()
                        );
                        for finding in &capture.findings {
                            println!("- {}", finding.note);
                        }
                    })
                }
                CapturesAction::Note {
                    id,
                    text,
                    evidence,
                    client,
                } => {
                    let remote = client.remote()?;
                    let finding = remote
                        .api
                        .add_finding(
                            &remote.token,
                            &id,
                            &NewFinding {
                                note: text,
                                evidence_artifact_id: evidence,
                            },
                        )
                        .await?;
                    client.show(&finding, |finding| println!("Noted {}", finding.id))
                }
                CapturesAction::Run {
                    url,
                    scenario,
                    revision,
                    width,
                    height,
                    color_scheme,
                    reduced_motion,
                    scope,
                } => {
                    run_capture(
                        scope,
                        RunCapture {
                            scenario,
                            url,
                            revision,
                            environment: Environment {
                                width,
                                height,
                                color_scheme,
                                reduced_motion,
                                ..Environment::default()
                            },
                            timeout_ms: None,
                            steps: Vec::new(),
                            allow_degraded: false,
                        },
                    )
                    .await
                }
            }
        })?
    }
}

async fn run_capture(scope: Scoped, request: RunCapture) -> Result<u8> {
    let remote = scope.client().remote()?;
    let capture = remote
        .api
        .run_capture(&remote.token, &scope.project()?, &request)
        .await?;
    scope.client().show(&capture, |capture| {
        println!("{} {} ({})", capture.id, capture.scenario, capture.status);
        if let Some(blocker) = &capture.blocker {
            println!("blocked: {blocker}");
        }
    })
}
