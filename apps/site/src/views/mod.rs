//! The pages' content: each view composes components and takes what it shows as arguments.

mod architecture;
mod docs;
mod home;
mod install;

pub use architecture::architecture_page;
pub use docs::docs_page;
pub use home::home_page;
pub use install::install_page;

pub const GITHUB_URL: &str = "https://github.com/Lucas-BRT/rupicola";
pub const RELEASES_URL: &str = "https://github.com/Lucas-BRT/rupicola/releases";
