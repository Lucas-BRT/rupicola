//! Local offline commands (`discover`, `scan`, `check`, `rules`, `modules`) and local panel
//! launcher (`serve`). None of these contact the backend or read client session tokens.

mod analysis;
mod panel;

pub use analysis::{CheckArgs, DiscoverArgs, ModulesArgs, RulesArgs, ScanArgs};
pub use panel::ServeArgs;
