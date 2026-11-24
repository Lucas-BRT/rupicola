//! The capture definitions a repository owns, kept as TOML files and pushed to the backend.

use super::client::{Scoped, rows};
use crate::{Result, block_on};
use anyhow::{Context, bail};
use rupicola_api::Scenario;
use rupicola_client::Api;
use std::path::{Path, PathBuf};

/// Where a repository keeps its scenarios, relative to its root.
const SCENARIO_DIR: &str = ".rupicola/scenarios";
/// Extension of a scenario file.
const SCENARIO_EXTENSION: &str = "toml";
/// How many characters of a hash `list` and `push` print.
const SHORT_HASH_CHARS: usize = 12;

#[derive(clap::Args)]
pub struct ScenariosArgs {
    #[command(subcommand)]
    action: ScenariosAction,
}

#[derive(clap::Subcommand)]
enum ScenariosAction {
    /// List a project's scenarios, the latest version of each.
    List(Scoped),
    /// Show one scenario as the backend holds it.
    Show {
        name: String,
        #[command(flatten)]
        scope: Scoped,
    },
    /// Read scenario files and report what is wrong with them, without a backend.
    Check {
        /// A scenario file, or a directory of them. Defaults to `.rupicola/scenarios`.
        path: Option<PathBuf>,
    },
    /// Push scenario files to the backend. Pushing a definition the backend already has changes
    /// nothing.
    Push {
        /// A scenario file, or a directory of them. Defaults to `.rupicola/scenarios`.
        path: Option<PathBuf>,
        #[command(flatten)]
        scope: Scoped,
    },
}

/// A scenario file and what it parsed to.
struct Loaded {
    path: PathBuf,
    scenario: Scenario,
}

impl Loaded {
    /// Reads and parses one scenario file.
    fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let scenario = Scenario::from_toml(&text)
            .with_context(|| format!("{} is not a valid scenario", path.display()))?;
        Ok(Self {
            path: path.to_owned(),
            scenario,
        })
    }

    /// Every scenario file at `path`: the file itself, or the `.toml` files of a directory in
    /// name order. Names must be unique, as the backend keeps one scenario per name.
    fn read_all(path: Option<&Path>) -> Result<Vec<Self>> {
        let path = path.unwrap_or_else(|| Path::new(SCENARIO_DIR));
        let mut files = if path.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(path)
                .with_context(|| format!("cannot read {}", path.display()))?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|file| {
                    file.extension()
                        .is_some_and(|extension| extension == SCENARIO_EXTENSION)
                })
                .collect();
            files.sort();
            files
        } else {
            vec![path.to_owned()]
        };
        if files.is_empty() {
            bail!("no .{SCENARIO_EXTENSION} files in {}", path.display());
        }
        let loaded = files
            .drain(..)
            .map(|file| Self::read(&file))
            .collect::<Result<Vec<_>>>()?;
        for (index, item) in loaded.iter().enumerate() {
            if let Some(other) = loaded[..index]
                .iter()
                .find(|other| other.scenario.name == item.scenario.name)
            {
                bail!(
                    "{} and {} both define the scenario `{}`",
                    other.path.display(),
                    item.path.display(),
                    item.scenario.name
                );
            }
        }
        Ok(loaded)
    }
}

fn short(hash: &str) -> &str {
    hash.get(..SHORT_HASH_CHARS).unwrap_or(hash)
}

impl ScenariosArgs {
    pub(crate) fn run(self) -> Result<u8> {
        match self.action {
            ScenariosAction::Check { path } => {
                for item in Loaded::read_all(path.as_deref())? {
                    println!(
                        "{}\t{}\t{} captures\t{}",
                        item.scenario.name,
                        short(&item.scenario.hash()),
                        item.scenario.matrix.cells(),
                        item.path.display()
                    );
                }
                Ok(0)
            }
            ScenariosAction::List(scope) => block_on(async {
                let remote = scope.client().remote()?;
                let scenarios = remote
                    .api
                    .scenarios(&remote.token, &scope.project()?)
                    .await?;
                scope.client().show(&scenarios, |scenarios| {
                    rows(scenarios, "No scenarios.", |scenario| {
                        format!(
                            "{}\t{}\t{} captures\t{}",
                            scenario.name,
                            short(&scenario.hash),
                            scenario.cells,
                            scenario.url
                        )
                    });
                })
            })?,
            ScenariosAction::Show { name, scope } => block_on(async {
                let remote = scope.client().remote()?;
                let version = remote
                    .api
                    .scenario(&remote.token, &scope.project()?, &name)
                    .await?;
                scope.client().show(&version, |version| {
                    println!("{} {}", version.scenario.name, short(&version.hash));
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&version.scenario).unwrap_or_default()
                    );
                })
            })?,
            ScenariosAction::Push { path, scope } => {
                let loaded = Loaded::read_all(path.as_deref())?;
                block_on(async {
                    let remote = scope.client().remote()?;
                    let project = scope.project()?;
                    let mut pushed = Vec::new();
                    for item in &loaded {
                        pushed.push(
                            remote
                                .api
                                .push_scenario(&remote.token, &project, &item.scenario)
                                .await
                                .with_context(|| format!("pushing {}", item.path.display()))?,
                        );
                    }
                    scope.client().show(&pushed, |pushed| {
                        for version in pushed {
                            println!("{}\t{}", version.scenario.name, short(&version.hash));
                        }
                    })
                })?
            }
        }
    }
}
