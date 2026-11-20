use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

#[derive(Clone, Copy)]
pub struct DocPage {
    pub slug: &'static str,
    pub group: &'static str,
    pub title: &'static str,
    pub source_path: &'static str,
    pub markdown: &'static str,
}

pub const DOCS: &[DocPage] = &[
    DocPage {
        slug: "overview",
        group: "Getting Started",
        title: "README & CLI Reference",
        source_path: "README.md",
        markdown: include_str!("../../README.md"),
    },
    DocPage {
        slug: "skill",
        group: "Getting Started",
        title: "SKILL.md Entrypoint",
        source_path: "SKILL.md",
        markdown: include_str!("../../SKILL.md"),
    },
    DocPage {
        slug: "common",
        group: "Core Rules",
        title: "Shared Rules (common.md)",
        source_path: "reference/common.md",
        markdown: include_str!("../../reference/common.md"),
    },
    DocPage {
        slug: "visual-verification",
        group: "Core Rules",
        title: "Visual Verification",
        source_path: "reference/visual-verification.md",
        markdown: include_str!("../../reference/visual-verification.md"),
    },
    DocPage {
        slug: "critique",
        group: "Core Rules",
        title: "Visual Critique",
        source_path: "reference/critique.md",
        markdown: include_str!("../../reference/critique.md"),
    },
    DocPage {
        slug: "architecture",
        group: "Core Rules",
        title: "Architecture Criteria",
        source_path: "reference/architecture.md",
        markdown: include_str!("../../reference/architecture.md"),
    },
    DocPage {
        slug: "workflows/plan",
        group: "Workflows",
        title: "Plan Workflow",
        source_path: "reference/workflows/plan.md",
        markdown: include_str!("../../reference/workflows/plan.md"),
    },
    DocPage {
        slug: "workflows/build",
        group: "Workflows",
        title: "Build Workflow",
        source_path: "reference/workflows/build.md",
        markdown: include_str!("../../reference/workflows/build.md"),
    },
    DocPage {
        slug: "workflows/review",
        group: "Workflows",
        title: "Review Workflow",
        source_path: "reference/workflows/review.md",
        markdown: include_str!("../../reference/workflows/review.md"),
    },
    DocPage {
        slug: "workflows/improve",
        group: "Workflows",
        title: "Improve Workflow",
        source_path: "reference/workflows/improve.md",
        markdown: include_str!("../../reference/workflows/improve.md"),
    },
    DocPage {
        slug: "ux/layout",
        group: "UX Principles",
        title: "Layout",
        source_path: "reference/ux/layout.md",
        markdown: include_str!("../../reference/ux/layout.md"),
    },
    DocPage {
        slug: "ux/typography",
        group: "UX Principles",
        title: "Typography",
        source_path: "reference/ux/typography.md",
        markdown: include_str!("../../reference/ux/typography.md"),
    },
    DocPage {
        slug: "ux/color",
        group: "UX Principles",
        title: "Color",
        source_path: "reference/ux/color.md",
        markdown: include_str!("../../reference/ux/color.md"),
    },
    DocPage {
        slug: "ux/motion",
        group: "UX Principles",
        title: "Motion",
        source_path: "reference/ux/motion.md",
        markdown: include_str!("../../reference/ux/motion.md"),
    },
    DocPage {
        slug: "ux/accessibility",
        group: "UX Principles",
        title: "Accessibility",
        source_path: "reference/ux/accessibility.md",
        markdown: include_str!("../../reference/ux/accessibility.md"),
    },
    DocPage {
        slug: "ux/content",
        group: "UX Principles",
        title: "Content & Copy",
        source_path: "reference/ux/content.md",
        markdown: include_str!("../../reference/ux/content.md"),
    },
    DocPage {
        slug: "environments/web",
        group: "Environments",
        title: "Web Environment",
        source_path: "reference/environments/web.md",
        markdown: include_str!("../../reference/environments/web.md"),
    },
    DocPage {
        slug: "environments/desktop",
        group: "Environments",
        title: "Desktop Environment",
        source_path: "reference/environments/desktop.md",
        markdown: include_str!("../../reference/environments/desktop.md"),
    },
    DocPage {
        slug: "environments/webview",
        group: "Environments",
        title: "Webview Environment",
        source_path: "reference/environments/webview.md",
        markdown: include_str!("../../reference/environments/webview.md"),
    },
    DocPage {
        slug: "frameworks/topcoat",
        group: "Frameworks",
        title: "Topcoat",
        source_path: "reference/frameworks/topcoat.md",
        markdown: include_str!("../../reference/frameworks/topcoat.md"),
    },
    DocPage {
        slug: "frameworks/iced",
        group: "Frameworks",
        title: "Iced",
        source_path: "reference/frameworks/iced.md",
        markdown: include_str!("../../reference/frameworks/iced.md"),
    },
    DocPage {
        slug: "frameworks/leptos",
        group: "Frameworks",
        title: "Leptos",
        source_path: "reference/frameworks/leptos.md",
        markdown: include_str!("../../reference/frameworks/leptos.md"),
    },
    DocPage {
        slug: "frameworks/dioxus",
        group: "Frameworks",
        title: "Dioxus",
        source_path: "reference/frameworks/dioxus.md",
        markdown: include_str!("../../reference/frameworks/dioxus.md"),
    },
    DocPage {
        slug: "frameworks/gpui",
        group: "Frameworks",
        title: "GPUI",
        source_path: "reference/frameworks/gpui.md",
        markdown: include_str!("../../reference/frameworks/gpui.md"),
    },
    DocPage {
        slug: "frameworks/gtk4",
        group: "Frameworks",
        title: "GTK4",
        source_path: "reference/frameworks/gtk4.md",
        markdown: include_str!("../../reference/frameworks/gtk4.md"),
    },
];

pub fn find_doc(slug: &str) -> Option<&'static DocPage> {
    let clean = slug.trim_matches('/').trim_end_matches(".md");
    if clean.is_empty() {
        return Some(&DOCS[0]);
    }
    DOCS.iter().find(|d| d.slug == clean)
}

fn rewrite_link(current_source: &str, dest: &str) -> Option<String> {
    if dest.starts_with("http://") || dest.starts_with("https://") || dest.starts_with('#') {
        return None;
    }
    let (path_part, frag) = match dest.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (dest, None),
    };
    if !path_part.ends_with(".md") {
        return None;
    }

    let base_dir = current_source
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let mut parts: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    for seg in path_part.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let resolved = parts.join("/");
    let found = DOCS.iter().find(|d| d.source_path == resolved)?;
    let mut out = format!("/docs/{}", found.slug);
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    Some(out)
}

pub fn render_markdown(doc: &DocPage) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(doc.markdown, opts).map(|ev| match ev {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let rewritten = rewrite_link(doc.source_path, &dest_url)
                .map(CowStr::from)
                .unwrap_or(dest_url);
            Event::Start(Tag::Link {
                link_type,
                dest_url: rewritten,
                title,
                id,
            })
        }
        other => other,
    });
    let mut html_out = String::with_capacity(doc.markdown.len() * 2);
    html::push_html(&mut html_out, parser);
    html_out
}
