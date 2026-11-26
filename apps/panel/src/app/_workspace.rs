//! The pages of the signed-in workspace. The layout below wraps each of them in the document,
//! top bar and navigation; a page only supplies its title and its content.

pub mod dashboard;
pub mod projects;
pub mod run;
pub mod settings;

use crate::{
    Account, Panel,
    components::layout::{Nav, document},
};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, context},
    metadata::Metadata,
    router::{Slot, layout, request},
    view::{View, view},
};

/// What every page's `<title>` ends with.
pub const TITLE_SUFFIX: &str = " · Rupicola";

/// A page title: `page` followed by the product name.
#[must_use]
pub fn title(page: &str) -> String {
    format!("{page}{TITLE_SUFFIX}")
}

/// Header sent by `panel.js` to request only the inner content fragment.
const PARTIAL_HEADER: &str = "x-rupicola-partial";

/// Whether the client requested a partial fragment instead of the full document shell.
#[must_use]
pub(crate) fn is_partial(cx: &Cx) -> bool {
    request::headers(cx)
        .get(PARTIAL_HEADER)
        .is_some_and(|value| value == "1")
}

#[layout(with(Metadata::new().title("Rupicola")))]
async fn shell(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let partial = is_partial(cx);
    let account = context::<Account>(cx);
    let panel = context::<Arc<Panel>>(cx);
    let active = Nav::of(request::uri(cx).path());
    Ok(view! {
        if partial {
            <main id="main" tabindex="-1">(slot)</main>
        } else {
            document(account: account, panel: panel, active: active, (slot))
        }
    })
}
