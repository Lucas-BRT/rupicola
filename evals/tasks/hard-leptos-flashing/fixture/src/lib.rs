use leptos::prelude::*;

const STYLE: &str = "
.badge-new { animation: blink 0.2s steps(1) infinite; }
@keyframes blink { 50% { opacity: 0; } }
.card { transition: transform 150ms ease-out; }
";

#[component]
pub fn Inbox() -> impl IntoView {
    view! {
        <style>{STYLE}</style>
        <div class="card"><span class="badge-new">"New"</span></div>
    }
}
