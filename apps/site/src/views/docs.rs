use super::{escape_html, layout};
use crate::content::docs::{DOCS, DocPage, render_markdown};
use std::fmt::Write;

pub fn render(doc: &DocPage, panel_url: &str) -> String {
    let mut sidebar_groups = String::new();
    let mut last_group = "";
    for item in DOCS {
        if item.group != last_group {
            if !last_group.is_empty() {
                sidebar_groups.push_str("</ul></div>");
            }
            last_group = item.group;
            let _ = write!(
                sidebar_groups,
                r#"<div><div class="docs-group-title">{}</div><ul class="docs-nav-list">"#,
                escape_html(item.group)
            );
        }
        let active_attr = if item.slug == doc.slug {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            sidebar_groups,
            r#"<li data-doc-item="{} {}"><a href="/docs/{}"{}>{}</a></li>"#,
            escape_html(item.title),
            escape_html(item.group),
            escape_html(item.slug),
            active_attr,
            escape_html(item.title)
        );
    }
    if !last_group.is_empty() {
        sidebar_groups.push_str("</ul></div>");
    }

    let rendered = render_markdown(doc);
    let body = format!(
        r#"<div class="container docs-layout">
  <aside class="docs-sidebar" aria-label="Documentation navigation">
    <input id="docs-filter" class="docs-search" type="search" placeholder="Filter docs..." aria-label="Filter documentation">
    {sidebar_groups}
  </aside>
  <article class="prose">
    <div class="breadcrumb">
      <span>Docs</span>
      <span aria-hidden="true">/</span>
      <span>{}</span>
      <span aria-hidden="true">/</span>
      <code>{}</code>
    </div>
    {rendered}
  </article>
</div>"#,
        escape_html(doc.group),
        escape_html(doc.source_path)
    );

    layout(
        doc.title,
        "Rupicola documentation, workflows, UX principles, and framework guides.",
        "docs",
        panel_url,
        &body,
    )
}
