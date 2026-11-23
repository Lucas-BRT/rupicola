//! The projects the signed-in account can see.

use super::client::{Client, rows};
use crate::{Result, block_on};
use rupicola_api::NewProject;
use rupicola_client::Api;

#[derive(clap::Args)]
pub struct ProjectsArgs {
    #[command(subcommand)]
    action: Option<ProjectsAction>,
    /// Without a subcommand: the same as `list`.
    #[command(flatten)]
    client: Client,
}

#[derive(clap::Subcommand)]
enum ProjectsAction {
    /// List the projects you can see.
    List(Client),
    /// Create a project; you become its owner.
    Create {
        /// Letters, digits, hyphens and underscores.
        id: String,
        /// Name shown to people.
        #[arg(long)]
        label: String,
        #[command(flatten)]
        client: Client,
    },
}

impl ProjectsArgs {
    pub(super) fn run(self) -> Result<u8> {
        block_on(async {
            match self.action.unwrap_or(ProjectsAction::List(self.client)) {
                ProjectsAction::List(client) => {
                    let remote = client.remote()?;
                    let projects = remote.api.projects(&remote.token).await?;
                    client.show(&projects, |projects| {
                        rows(projects, "No projects.", |project| {
                            let role = project.role.map_or("-".to_owned(), |role| role.to_string());
                            format!("{}\t{}\t{role}", project.id, project.label)
                        });
                    })
                }
                ProjectsAction::Create { id, label, client } => {
                    let remote = client.remote()?;
                    let project = remote
                        .api
                        .create_project(&remote.token, &NewProject { id, label })
                        .await?;
                    client.show(&project, |project| {
                        println!("Created project {}", project.id);
                    })
                }
            }
        })?
    }
}
