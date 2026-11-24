//! The problems found in a project's interface.

use super::client::{Body, Client, Scoped, choice, rows};
use crate::{Result, block_on};
use rupicola_api::{NewComment, NewProblem, ProblemPatch, ProblemStatus, Severity};
use rupicola_client::Api;

#[derive(clap::Args)]
pub struct ProblemsArgs {
    #[command(subcommand)]
    action: ProblemsAction,
}

#[derive(clap::Subcommand)]
enum ProblemsAction {
    /// List a project's problems, most recently changed first.
    List(Scoped),
    /// Show one problem with its rules, evidence and comments.
    Show {
        id: String,
        #[command(flatten)]
        client: Client,
    },
    /// Open a problem from a JSON file shaped like the API's `NewProblem`.
    Create {
        #[command(flatten)]
        scope: Scoped,
        #[command(flatten)]
        body: Body,
    },
    /// Change a problem's status, severity or proposed solution.
    Update {
        id: String,
        #[arg(long, value_parser = choice::<ProblemStatus>(ProblemStatus::NAMES))]
        status: Option<ProblemStatus>,
        #[arg(long, value_parser = choice::<Severity>(Severity::NAMES))]
        severity: Option<Severity>,
        /// The solution to propose.
        #[arg(long)]
        solution: Option<String>,
        #[command(flatten)]
        client: Client,
    },
    /// Add a comment to a problem.
    Comment {
        id: String,
        text: String,
        #[command(flatten)]
        client: Client,
    },
    /// Infer and file problems from a capture's findings.
    Analyze {
        capture: String,
        #[command(flatten)]
        scope: Scoped,
    },
}

impl ProblemsArgs {
    pub(super) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action {
                ProblemsAction::List(scope) => {
                    let remote = scope.client().remote()?;
                    let problems = remote
                        .api
                        .problems(&remote.token, &scope.project()?)
                        .await?;
                    scope.client().show(&problems, |problems| {
                        rows(problems, "No problems.", |problem| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                problem.id, problem.status, problem.severity, problem.title
                            )
                        });
                    })
                }
                ProblemsAction::Show { id, client } => {
                    let remote = client.remote()?;
                    let problem = remote.api.problem(&remote.token, &id).await?;
                    client.show(&problem, |problem| {
                        println!(
                            "{} {} ({}, {})",
                            problem.id, problem.title, problem.status, problem.severity
                        );
                        if !problem.diagnosis.is_empty() {
                            println!("{}", problem.diagnosis);
                        }
                        if !problem.proposed_solution.is_empty() {
                            println!("proposed: {}", problem.proposed_solution);
                        }
                        for comment in &problem.comments {
                            println!("- {}", comment.text);
                        }
                    })
                }
                ProblemsAction::Create { scope, body } => {
                    let form: NewProblem = body.read()?;
                    let remote = scope.client().remote()?;
                    let problem = remote
                        .api
                        .create_problem(&remote.token, &scope.project()?, &form)
                        .await?;
                    scope
                        .client()
                        .show(&problem, |problem| println!("Opened {}", problem.id))
                }
                ProblemsAction::Update {
                    id,
                    status,
                    severity,
                    solution,
                    client,
                } => {
                    let remote = client.remote()?;
                    let problem = remote
                        .api
                        .update_problem(
                            &remote.token,
                            &id,
                            &ProblemPatch {
                                status,
                                severity,
                                proposed_solution: solution,
                            },
                        )
                        .await?;
                    client.show(&problem, |problem| {
                        println!("{} is now {}", problem.id, problem.status);
                    })
                }
                ProblemsAction::Comment { id, text, client } => {
                    let remote = client.remote()?;
                    let comment = remote
                        .api
                        .comment_problem(&remote.token, &id, &NewComment { text })
                        .await?;
                    client.show(&comment, |comment| println!("Commented {}", comment.id))
                }
                ProblemsAction::Analyze { capture, scope } => {
                    let remote = scope.client().remote()?;
                    let created = remote
                        .api
                        .analyze_capture(&remote.token, &scope.project()?, &capture)
                        .await?;
                    scope.client().show(&created, |problems| {
                        rows(problems, "No new problems.", |problem| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                problem.id, problem.status, problem.severity, problem.title
                            )
                        });
                    })
                }
            }
        })?
    }
}
