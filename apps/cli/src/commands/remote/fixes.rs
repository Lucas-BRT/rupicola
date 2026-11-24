//! The fixes made for a project's problems.

use super::client::{Body, Client, Scoped, choice, rows};
use crate::{Result, block_on};
use rupicola_api::{FixStatus, NewFix, NewReview, NewStep};
use rupicola_client::Api;

#[derive(clap::Args)]
pub struct FixesArgs {
    #[command(subcommand)]
    action: FixesAction,
}

#[derive(clap::Subcommand)]
enum FixesAction {
    /// List a project's fixes, most recently changed first.
    List(Scoped),
    /// Show one fix with its steps, comparisons and reviews.
    Show {
        id: String,
        #[command(flatten)]
        client: Client,
    },
    /// Record a fix from a JSON file shaped like the API's `NewFix`.
    Create {
        #[command(flatten)]
        scope: Scoped,
        #[command(flatten)]
        body: Body,
    },
    /// Record one verified step of a fix, from a JSON file shaped like `NewStep`.
    Step {
        id: String,
        #[command(flatten)]
        body: Body,
        #[command(flatten)]
        client: Client,
    },
    /// Review a fix.
    Review {
        id: String,
        /// The verdict.
        #[arg(long, value_parser = choice::<FixStatus>(FixStatus::NAMES))]
        outcome: FixStatus,
        #[arg(long, default_value = "")]
        comment: String,
        #[command(flatten)]
        client: Client,
    },
}

impl FixesArgs {
    pub(crate) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action {
                FixesAction::List(scope) => {
                    let remote = scope.client().remote()?;
                    let fixes = remote.api.fixes(&remote.token, &scope.project()?).await?;
                    scope.client().show(&fixes, |fixes| {
                        rows(fixes, "No fixes.", |fix| {
                            format!(
                                "{}\t{}\t{} problems\t{}",
                                fix.id, fix.status, fix.problems, fix.title
                            )
                        });
                    })
                }
                FixesAction::Show { id, client } => {
                    let remote = client.remote()?;
                    let fix = remote.api.fix(&remote.token, &id).await?;
                    client.show(&fix, |fix| {
                        println!("{} {} ({})", fix.id, fix.title, fix.status);
                        if !fix.summary.is_empty() {
                            println!("{}", fix.summary);
                        }
                        for step in &fix.steps {
                            println!("- {}", step.step.title);
                        }
                        for review in &fix.reviews {
                            println!("review: {} {}", review.outcome, review.comment);
                        }
                    })
                }
                FixesAction::Create { scope, body } => {
                    let form: NewFix = body.read()?;
                    let remote = scope.client().remote()?;
                    let fix = remote
                        .api
                        .create_fix(&remote.token, &scope.project()?, &form)
                        .await?;
                    scope
                        .client()
                        .show(&fix, |fix| println!("Recorded {}", fix.id))
                }
                FixesAction::Step { id, body, client } => {
                    let form: NewStep = body.read()?;
                    let remote = client.remote()?;
                    let step = remote.api.add_step(&remote.token, &id, &form).await?;
                    client.show(&step, |step| println!("Recorded step {}", step.step.title))
                }
                FixesAction::Review {
                    id,
                    outcome,
                    comment,
                    client,
                } => {
                    let remote = client.remote()?;
                    let fix = remote
                        .api
                        .review_fix(&remote.token, &id, &NewReview { outcome, comment })
                        .await?;
                    client.show(&fix, |fix| println!("{} is now {}", fix.id, fix.status))
                }
            }
        })?
    }
}
