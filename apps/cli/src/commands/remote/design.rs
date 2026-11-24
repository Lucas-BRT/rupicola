//! The design records of a project.

use super::client::{Body, Scoped, rows};
use crate::{Result, block_on};
use rupicola_api::NewDesignEntry;
use rupicola_client::Api;

#[derive(clap::Args)]
pub struct DesignArgs {
    #[command(subcommand)]
    action: DesignAction,
}

#[derive(clap::Subcommand)]
enum DesignAction {
    /// List a project's design records, newest first.
    List(Scoped),
    /// Record a brief, direction, tactic or decision from a JSON file shaped like the API's
    /// `NewDesignEntry`.
    Add {
        #[command(flatten)]
        scope: Scoped,
        #[command(flatten)]
        body: Body,
    },
}

impl DesignArgs {
    pub(crate) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action {
                DesignAction::List(scope) => {
                    let remote = scope.client().remote()?;
                    let entries = remote.api.design(&remote.token, &scope.project()?).await?;
                    scope.client().show(&entries, |entries| {
                        rows(entries, "No design records.", |record| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                record.id,
                                record.entry.kind,
                                record.entry.status,
                                record.entry.title
                            )
                        });
                    })
                }
                DesignAction::Add { scope, body } => {
                    let form: NewDesignEntry = body.read()?;
                    let remote = scope.client().remote()?;
                    let entry = remote
                        .api
                        .add_design(&remote.token, &scope.project()?, &form)
                        .await?;
                    scope
                        .client()
                        .show(&entry, |entry| println!("Recorded {}", entry.id))
                }
            }
        })?
    }
}
