use leptos::prelude::*;

#[component]
pub fn DeleteDialog(on_ok: Callback<()>, on_cancel: Callback<()>) -> impl IntoView {
    view! {
        <div role="dialog">
            <h2>"Warning"</h2>
            <p>"Are you sure?"</p>
            <button autofocus on:click=move |_| on_ok.run(())>"OK"</button>
            <button on:click=move |_| on_cancel.run(())>"Cancel"</button>
        </div>
    }
}
