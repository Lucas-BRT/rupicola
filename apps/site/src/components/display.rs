//! Pieces the pages are made of: sections, cards, buttons and code blocks.

use topcoat::{
    Result,
    view::{Child, View, component, view},
};

/// A page section; `child` is its content, which `container` wraps in a grid class.
#[component]
pub async fn section(#[default] grid: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <section class="section">
            <div class=(format!("container {grid}").trim_end().to_owned())>(child)</div>
        </section>
    })
}

/// A bordered card.
#[component]
pub async fn card(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <div class="card">(child)</div> })
}

/// A card that is an `<article>`.
#[component]
pub async fn article_card(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <article class="card">(child)</article> })
}

/// The small caps line above a heading.
#[component]
pub async fn eyebrow(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <p class="eyebrow">(child)</p> })
}

/// A button-styled link; `external` opens it in a new tab.
#[component]
pub async fn button(
    #[into] href: String,
    #[default] primary: bool,
    #[default] external: bool,
    #[default] download: Option<&str>,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a class=(if primary { "btn btn-primary" } else { "btn" }) href=(href)
            if external { target="_blank" rel="noopener" }
            if let Some(name) = download { download=(name) }>(child)</a>
    })
}

/// A labelled code listing with a button that copies it.
#[component]
pub async fn code_block(label: &str, code: &str) -> Result<impl View> {
    Ok(view! {
        <div class="code-block">
            <div class="code-head">
                <span>(label)</span>
                <button class="btn btn-sm" type="button" data-copy=(code)>"Copy"</button>
            </div>
            <pre><code>(code)</code></pre>
        </div>
    })
}

/// One stage of the pipeline strip.
#[component]
pub async fn stage_cell(number: &str, name: &str, description: &str) -> Result<impl View> {
    Ok(view! {
        <div class="stage-cell">
            <span class="stage-num">(number)</span>
            <strong>(name)</strong>
            <p>(description)</p>
        </div>
    })
}

/// The pipeline from capture to storage, under the hero.
#[component]
pub async fn stage_strip() -> Result<impl View> {
    const STAGES: [(&str, &str, &str); 6] = [
        (
            "01",
            "Captures",
            "Headless Playwright matrix across viewports, themes, and routes.",
        ),
        (
            "02",
            "Problems",
            "Structured queue with live agent claims and task progress.",
        ),
        (
            "03",
            "Fixes",
            "Before/after PNG & MP4 evidence with Split Slider and Pixel Diff.",
        ),
        (
            "04",
            "Proposes",
            "Side-by-side HTML/image visual directions for design review.",
        ),
        (
            "05",
            "Design",
            "Persistent art direction, palette, tactics, and decision log.",
        ),
        (
            "06",
            "Storage",
            "By-Fix/Date/Run disk breakdown with S3, R2, and rclone offload.",
        ),
    ];
    Ok(view! {
        for (number, name, description) in STAGES {
            stage_cell(number: number, name: name, description: description)
        }
    })
}

/// One tag per UI framework the skill covers.
#[component]
pub async fn framework_tags() -> Result<impl View> {
    const FRAMEWORKS: [&str; 13] = [
        "Topcoat",
        "Leptos",
        "Dioxus",
        "Yew",
        "Iced",
        "GPUI",
        "egui",
        "Slint",
        "GTK4",
        "libadwaita",
        "Relm4",
        "libcosmic",
        "Tauri",
    ];
    Ok(view! {
        for framework in FRAMEWORKS {
            <span class="tag">(framework)</span>
        }
    })
}
