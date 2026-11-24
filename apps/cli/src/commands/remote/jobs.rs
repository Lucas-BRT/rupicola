//! Capture jobs: asking a runner to capture a scenario on trees of the code.

use super::client::{Client, Scoped, rows};
use crate::{Result, block_on};
use rupicola_api::{Job, NewJob, TreeSpec};
use rupicola_client::Api;
use std::str::FromStr;

/// Separates a tree's base ref from the refs merged onto it.
const MERGE_SEPARATOR: char = '+';
/// Separates a tree's name from its refs.
const NAME_SEPARATOR: char = '=';

/// A tree written `NAME=BASE` or `NAME=BASE+MERGE+MERGE`, such as `fixed=develop+fix/spacing`.
#[derive(Clone, Debug)]
struct TreeArg(TreeSpec);

impl FromStr for TreeArg {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (name, refs) = text
            .split_once(NAME_SEPARATOR)
            .ok_or_else(|| format!("`{text}` is not NAME=BASE[+MERGE...]"))?;
        let mut refs = refs.split(MERGE_SEPARATOR).map(str::to_owned);
        let base = refs.next().filter(|base| !base.is_empty());
        Ok(Self(TreeSpec {
            name: name.to_owned(),
            base: base.ok_or_else(|| format!("`{text}` names no base ref"))?,
            merge: refs.collect(),
            fixes: Vec::new(),
        }))
    }
}

#[derive(clap::Args)]
pub struct JobsArgs {
    #[command(subcommand)]
    action: JobsAction,
}

#[derive(clap::Subcommand)]
enum JobsAction {
    /// List a project's capture jobs, newest first.
    List {
        /// Only the jobs waiting or running.
        #[arg(long)]
        open: bool,
        #[command(flatten)]
        scope: Scoped,
    },
    /// Show one job: its progress and what it made.
    Show {
        id: String,
        #[command(flatten)]
        client: Client,
    },
    /// Queue a job: run a pushed scenario on one or more trees of the code.
    Create {
        /// Name of a scenario the project has.
        scenario: String,
        /// A tree to capture, as NAME=BASE or NAME=BASE+MERGE; repeat for more.
        #[arg(long = "tree", required = true)]
        trees: Vec<TreeArg>,
        /// Exact scenario version; the latest when absent.
        #[arg(long)]
        hash: Option<String>,
        /// The fix the captures are for.
        #[arg(long)]
        fix: Option<String>,
        #[command(flatten)]
        scope: Scoped,
    },
    /// Cancel a job that has not finished.
    Cancel {
        id: String,
        #[command(flatten)]
        client: Client,
    },
}

/// One line about a job.
fn line(job: &Job) -> String {
    format!(
        "{}\t{}\t{}/{}\t{}",
        job.id, job.status, job.cells_done, job.cells_total, job.scenario
    )
}

impl JobsArgs {
    pub(crate) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action {
                JobsAction::List { open, scope } => {
                    let remote = scope.client().remote()?;
                    let jobs = remote
                        .api
                        .jobs(&remote.token, &scope.project()?, open)
                        .await?;
                    scope.client().show(&jobs, |jobs| {
                        rows(jobs, "No jobs.", line);
                    })
                }
                JobsAction::Show { id, client } => {
                    let remote = client.remote()?;
                    let job = remote.api.job(&remote.token, &id).await?;
                    client.show(&job, |job| {
                        println!("{}", line(job));
                        if let Some(runner) = &job.runner {
                            println!("runner: {runner}");
                        }
                        if let Some(error) = &job.error {
                            println!("error: {error}");
                        }
                        for capture in &job.captures {
                            println!(
                                "{}\t{}\t{}\t{}",
                                capture.capture, capture.tree, capture.viewport, capture.theme
                            );
                        }
                    })
                }
                JobsAction::Create {
                    scenario,
                    trees,
                    hash,
                    fix,
                    scope,
                } => {
                    let remote = scope.client().remote()?;
                    let form = NewJob {
                        scenario,
                        hash,
                        trees: trees.into_iter().map(|tree| tree.0).collect(),
                        fix,
                    };
                    let job = remote
                        .api
                        .create_job(&remote.token, &scope.project()?, &form)
                        .await?;
                    scope
                        .client()
                        .show(&job, |job| println!("Queued {}", line(job)))
                }
                JobsAction::Cancel { id, client } => {
                    let remote = client.remote()?;
                    let job = remote.api.cancel_job(&remote.token, &id).await?;
                    client.show(&job, |job| println!("{}", line(job)))
                }
            }
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_reads_its_name_base_and_merges() {
        let tree: TreeArg = "fixed=develop+fix/a+fix/b".parse().unwrap();
        assert_eq!(tree.0.name, "fixed");
        assert_eq!(tree.0.base, "develop");
        assert_eq!(tree.0.merge, ["fix/a", "fix/b"]);
        let base: TreeArg = "base=develop".parse().unwrap();
        assert_eq!(base.0.merge.len(), 0);
        for bad in ["develop", "base=", "base=+fix"] {
            assert!(bad.parse::<TreeArg>().is_err(), "{bad}");
        }
    }
}
