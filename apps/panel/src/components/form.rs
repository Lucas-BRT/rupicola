//! Form building blocks.

use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Attributes, Child, View, component, view},
};

/// The hidden field every state-changing form sends back.
#[component]
pub async fn csrf(cx: &Cx, token: &str) -> Result<impl View> {
    Ok(view! { <input type="hidden" name="csrf" value=(token)> })
}

/// A labelled one-line input; `attrs` goes on the `<input>`.
#[component]
pub async fn field(
    cx: &Cx,
    #[into] id: String,
    name: &str,
    label: &str,
    #[default] attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <label for=(&id)>(label)</label>
        <input id=(&id) name=(name) (attrs)>
    })
}

/// A labelled text area holding `value`; `attrs` goes on the `<textarea>`.
#[component]
pub async fn text_area(
    cx: &Cx,
    #[into] id: String,
    name: &str,
    label: &str,
    #[default] value: &str,
    #[default] attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <label for=(&id)>(label)</label>
        <textarea id=(&id) name=(name) (attrs)>(value)</textarea>
    })
}

/// A labelled select; `child` holds the `<option>`s.
#[component]
pub async fn select_field(
    cx: &Cx,
    #[into] id: String,
    name: &str,
    label: &str,
    #[default] attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <label for=(&id)>(label)</label>
        <select id=(&id) name=(name) (attrs)>(child)</select>
    })
}

/// A form that posts to `action`, with the CSRF field first.
#[component]
pub async fn stack_form(
    cx: &Cx,
    #[into] action: String,
    token: &str,
    #[default] attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <form class="stack-form" method="post" action=(action) (attrs)>
            csrf(token: token)
            (child)
        </form>
    })
}
