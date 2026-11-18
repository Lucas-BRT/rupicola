use leptos::prelude::*;

#[component]
pub fn Search() -> impl IntoView {
    view! {
        <div class="card transition-all hover:scale-105">
            <input class="outline-none border p-2" type="text" placeholder="Search"/>
        </div>
    }
}
