//! Server-side implementation of the atomic `rupicola_api` operation traits
//! backed directly by [`crate::services`] and [`rupicola_core`].
//!
//! Because supertraits (`WorkspaceOps`, `AuditorOps`, `FullAgentOps`) have blanket
//! implementations, `ServerOps` automatically satisfies them once the atomic
//! traits are implemented here.

mod automation;
mod domain;
mod governance;
mod media;

use crate::{
    domain::{ApiError, Principal},
    state::Shared,
};
use rupicola_api::OpsBase;

/// Most bytes one `artifact_read` may return.
pub(crate) const MAX_READ_BYTES: usize = 1024 * 1024;

/// Executes every atomic operation trait in-process against the server's services.
pub(crate) struct ServerOps {
    pub(crate) state: Shared,
    pub(crate) principal: Principal,
}

impl ServerOps {
    pub(crate) const fn new(state: Shared, principal: Principal) -> Self {
        Self { state, principal }
    }
}

impl OpsBase for ServerOps {
    type Error = ApiError;
}
