//! Local commands (`discover`, `scan`, `check`, `rules`, `modules`) and panel launcher
//! (`serve`). None of these read client session tokens.

mod analysis;
mod panel;

pub use analysis::{CheckArgs, DiscoverArgs, ModulesArgs, RulesArgs, ScanArgs};
pub use panel::ServeArgs;
