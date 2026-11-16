// view! { <div onclick="x" class="bg-gradient-to-r from-violet-600 to-fuchsia-500"></div> }
use topcoat::view::{class, view};

const LABEL: StaticClass = class!("text-xs tracking-[0.12em] uppercase");

#[page]
async fn home(cx: &Cx) -> Result<impl View> {
    let brace = "{ not markup (";
    Ok(view! {
        <section class="transition-all">
            <h1 class="bg-clip-text">"Hello {"</h1>
            <p class=(class!("size-3 rounded-full", tone))>"x"</p>
        </section>
    })
}
