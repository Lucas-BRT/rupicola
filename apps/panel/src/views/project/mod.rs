//! The project's pages: the frame with its tabs, and one view per tab.

pub mod captures;
pub mod design;
pub mod fixes;
pub mod overview;
pub mod problems;
pub mod settings;

pub use captures::render_captures;
pub use fixes::render_fixes;
pub use problems::render_problems;

use crate::components::badge::{BadgeVariant, badge};
use crate::project::AssetOption;
use crate::{
    Account, Cx, PageResult, Panel, Run, auth, design_record, fix_record, problem_record, view,
};
use auth::Role;
use fix_record::{ComparisonPair, FixRecord, FixStatus};
use problem_record::{Problem, ProblemStatus, Severity};

pub(super) fn severity_badge(sev: Severity) -> BadgeVariant {
    match sev {
        Severity::Critical => BadgeVariant::Destructive,
        Severity::Major => BadgeVariant::Primary,
        Severity::Minor | Severity::Polish => BadgeVariant::Secondary,
    }
}

pub(super) fn problem_status_badge(st: ProblemStatus) -> BadgeVariant {
    match st {
        ProblemStatus::Open | ProblemStatus::NeedsReprocess => BadgeVariant::Destructive,
        ProblemStatus::InProgress => BadgeVariant::Primary,
        ProblemStatus::Resolved | ProblemStatus::Intentional | ProblemStatus::FalsePositive => {
            BadgeVariant::Secondary
        }
    }
}

pub(super) fn fix_status_badge(st: FixStatus) -> BadgeVariant {
    match st {
        FixStatus::Approved => BadgeVariant::Primary,
        FixStatus::ChangesRequested => BadgeVariant::Destructive,
        FixStatus::PendingReview | FixStatus::Superseded => BadgeVariant::Secondary,
    }
}

pub(super) fn is_video_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    matches!(path.rsplit_once('.'), Some((_, "mp4" | "webm")))
}

pub(super) fn extract_media_url_from_text(text: &str) -> Option<String> {
    let idx = text.find("/media/")?;
    let token: String = text[idx..]
        .chars()
        .take_while(|ch| !ch.is_whitespace())
        .collect();
    let clean = token.trim_end_matches(['.', ',', ';', ')']);
    if matches!(clean.rsplit_once('.'), Some((_, "mp4" | "webm" | "png"))) {
        Some(clean.to_string())
    } else {
        None
    }
}

pub(super) fn effective_after_url(fix: &FixRecord, pair: &ComparisonPair) -> String {
    if is_video_url(&pair.after_url) {
        return pair.after_url.clone();
    }
    for rev in fix.reviews.iter().rev() {
        if let Some(u) = extract_media_url_from_text(&rev.comment)
            && is_video_url(&u)
        {
            return u;
        }
    }
    for st in fix.steps.iter().rev() {
        if let Some(ev) = &st.evidence_url
            && is_video_url(ev)
        {
            return ev.clone();
        }
    }
    pair.after_url.clone()
}

/// A tab of the project page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Captures,
    Problems,
    Fixes,
    Design,
    Settings,
}

impl Tab {
    const ALL: [Self; 6] = [
        Self::Overview,
        Self::Captures,
        Self::Problems,
        Self::Fixes,
        Self::Design,
        Self::Settings,
    ];

    const fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Captures => "captures",
            Self::Problems => "problems",
            Self::Fixes => "fixes",
            Self::Design => "design",
            Self::Settings => "settings",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Captures => "Captures",
            Self::Problems => "Problems",
            Self::Fixes => "Fixes",
            Self::Design => "Design",
            Self::Settings => "Settings",
        }
    }

    const fn heading(self) -> &'static str {
        match self {
            Self::Settings => "Project settings",
            other => other.label(),
        }
    }
}

/// The project page around one tab's `body`: breadcrumbs, heading and tabs.
///
/// # Errors
///
/// The status code when the page cannot be rendered.
pub async fn frame<'a>(
    cx: &'a Cx,
    account: &'a Account,
    panel: &Panel,
    project: &'a crate::project::Context,
    tab: Tab,
    body: topcoat::view::Child<'a>,
) -> PageResult<topcoat::router::content::Html<String>> {
    let (id, label, base, role) = (&project.id, &project.label, &project.base, project.role);
    let overview = tab == Tab::Overview;
    let page_title = if overview {
        label.clone()
    } else {
        format!("{} · {label}", tab.heading())
    };
    let content = shell!(cx, &page_title, account, panel, "projects", {
        <nav class="breadcrumbs" aria-label="Breadcrumb"><a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span><span>(label)</span></nav>
        <header class="page-title project-masthead">
            <div><h1>(if overview { label.as_str() } else { tab.heading() })</h1>
                if overview && label != id { <code class="muted">(id)</code> }
            </div>
        </header>
        <nav class="tabs" aria-label="Project">
            for each in Tab::ALL {
                if each != Tab::Settings || role == Role::Owner {
                    <a href=(if each == Tab::Overview { base.clone() } else { format!("{base}/{}", each.slug()) })
                        if tab == each { aria-current="page" }>(each.label())</a>
                }
            }
        </nav>
        (body)
    });
    Box::pin(auth::page(cx, content)).await
}
