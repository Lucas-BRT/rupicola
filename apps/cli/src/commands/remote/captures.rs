//! The captures of a project and what reviewers noted on them.

use super::client::{Client, Scoped, rows};
use crate::{Result, block_on};
use rupicola_api::NewFinding;
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
            }
        })?
    }
}
