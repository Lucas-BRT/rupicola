//! The project's pages: the frame with its tabs, and one view per tab.

pub mod captures;
pub mod design;
pub mod fixes;
pub mod overview;
pub mod problems;
pub mod settings;

pub use captures::captures_tab;
pub use fixes::fixes_tab;
pub use problems::problems_tab;

use crate::components::badge::{BadgeVariant, badge};
use crate::components::layout::{breadcrumbs, tab_link, tabs};
use crate::project::AssetOption;
use crate::records::{FixRecord, Problem};
use crate::{Run, auth};
use auth::Role;
use rupicola_api::{DesignEntry, FixStatus, Job, JobStatus, ProblemStatus, Severity};
use topcoat::{
    Result,
    context::Cx,
    view::{Child, View, component, view},
};

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
    /// The tab a page at `path` belongs to, for a project served at `base`.
    #[must_use]
    pub fn of(path: &str, base: &str) -> Self {
        let rest = path
            .strip_prefix(base)
            .unwrap_or_default()
            .trim_start_matches('/');
        match rest.split('/').next().unwrap_or_default() {
            "captures" | "runs" | "images" | "videos" => Self::Captures,
            "problems" | "findings" => Self::Problems,
            "fixes" => Self::Fixes,
            "design" => Self::Design,
            "settings" => Self::Settings,
            _ => Self::Overview,
        }
    }

    pub const ALL: [Self; 6] = [
        Self::Overview,
        Self::Captures,
        Self::Problems,
        Self::Fixes,
        Self::Design,
        Self::Settings,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Captures => "captures",
            Self::Problems => "problems",
            Self::Fixes => "fixes",
            Self::Design => "design",
            Self::Settings => "settings",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Captures => "Captures",
            Self::Problems => "Problems",
            Self::Fixes => "Fixes",
            Self::Design => "Design",
            Self::Settings => "Settings",
        }
    }

    pub const fn heading(self) -> &'static str {
        match self {
            Self::Settings => "Project settings",
            other => other.label(),
        }
    }
}

/// The project page around one tab: breadcrumbs, heading and tabs; `child` is the tab.
#[component]
pub async fn project_frame(
    cx: &Cx,
    project: &crate::project::ProjectMeta,
    tab: Tab,
    open_jobs: Vec<Job>,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let (id, label, base, role) = (&project.id, &project.label, &project.base, project.role);
    let overview = tab == Tab::Overview;
    let jobs_url = format!("{base}/jobs");
    let done: u32 = open_jobs.iter().map(|j| j.cells_done).sum();
    let total: u32 = open_jobs.iter().map(|j| j.cells_total).sum();
    let job_noun = if open_jobs.len() == 1 {
        "capture job"
    } else {
        "capture jobs"
    };
    let job_verb = if open_jobs.iter().any(|j| j.status == JobStatus::Running) {
        "running"
    } else {
        "queued"
    };
    let jobs_summary = format!(
        "{} {job_noun} {job_verb} · {done}/{total} cells",
        open_jobs.len()
    );
    Ok(view! {
        breadcrumbs(<a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span><span>(label)</span>)
        <header class="page-title project-masthead">
            <div><h1>(if overview { label.as_str() } else { tab.heading() })</h1>
                if overview && label != id { <code class="muted">(id)</code> }
            </div>
            <div class="jobs-indicator" role="status" aria-live="polite" data-jobs-url=(jobs_url) if open_jobs.is_empty() { hidden="" }>(jobs_summary)</div>
        </header>
        tabs(label: "Project",
            for each in Tab::ALL {
                if each != Tab::Settings || role == Role::Owner {
                    tab_link(
                        href: if each == Tab::Overview { base.clone() } else { format!("{base}/{}", each.slug()) },
                        current: tab == each,
                        (each.label())
                    )
                }
            }
        )
        (child)
    })
}
