use crate::components::page::{Nav, shell};
use crate::content::docs::{DOCS, DocPage, render_markdown};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Unescaped, View, component, view},
};

/// A documentation page with the sidebar listing every document.
#[component]
pub async fn docs_page(cx: &Cx, doc: &DocPage, panel_url: &str) -> Result<impl View> {
    let mut groups: Vec<(&str, Vec<&DocPage>)> = Vec::new();
    for item in DOCS {
        match groups.last_mut() {
            Some((group, items)) if *group == item.group => items.push(item),
            _ => groups.push((item.group, vec![item])),
        }
    }
    let rendered = Unescaped::new_unchecked(render_markdown(doc));
    Ok(view! {
        shell(
            title: doc.title,
            description: "Rupicola documentation, workflows, UX principles, and framework guides.",
            active: Nav::Docs,
            panel_url: panel_url,
            <div class="container docs-layout">
                <aside class="docs-sidebar" aria-label="Documentation navigation">
                    <input id="docs-filter" class="docs-search" type="search" placeholder="Filter docs..." aria-label="Filter documentation">
                    for (group, items) in &groups {
                        <div>
                            <div class="docs-group-title">(group)</div>
                            <ul class="docs-nav-list">
                                for item in items {
                                    <li data-doc-item=(format!("{} {}", item.title, item.group))>
                                        <a href=(format!("/docs/{}", item.slug)) if item.slug == doc.slug { aria-current="page" }>(item.title)</a>
                                    </li>
                                }
                            </ul>
                        </div>
                    }
                </aside>
                <article class="prose">
                    <div class="breadcrumb">
                        <span>"Docs"</span>
                        <span aria-hidden="true">"/"</span>
                        <span>(doc.group)</span>
                        <span aria-hidden="true">"/"</span>
                        <code>(doc.source_path)</code>
                    </div>
                    (rendered)
                </article>
            </div>
        )
    })
}
