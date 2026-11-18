use leptos::prelude::*;

#[component]
pub fn EmailField(valid: ReadSignal<bool>) -> impl IntoView {
    view! {
        <input
            type="text"
            placeholder="Email"
            class=move || if valid.get() { "border-gray-300" } else { "border-red-500" }
        />
    }
}
