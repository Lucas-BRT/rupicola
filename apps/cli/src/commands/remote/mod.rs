//! Commands that authenticate and talk to a Rupicola backend (`login`, `publish`,
//! `mcp-bridge`, `projects`, `captures`, `problems`, `fixes`, `design`).

mod account;
mod bridge;
mod captures;
mod client;
mod design;
mod fixes;
mod problems;
mod projects;
mod scenarios;

pub use account::{LoginArgs, Logout, WhoamiArgs, set_token};
pub use bridge::{McpBridgeArgs, PublishArgs};
pub use captures::CapturesArgs;
pub use design::DesignArgs;
pub use fixes::FixesArgs;
pub use problems::ProblemsArgs;
pub use projects::ProjectsArgs;
pub use scenarios::ScenariosArgs;
