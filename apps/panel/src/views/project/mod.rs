//! The project's pages: the frame with its tabs, and one view per tab.

pub mod captures;
pub mod design;
pub mod fixes;
pub mod overview;
pub mod problems;
pub mod settings;

pub use captures::captures_tab;
pub use fixes::{fix_detail, fixes_tab};
pub use problems::{problem_detail, problems_tab};

use crate::components::badge::{BadgeVariant, badge};
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
            Self::Settings => "Project settings",
        }
    }

    pub const fn heading(self) -> &'static str {
        match self {
            Self::Settings => "Project settings",
            other => other.label(),
        }
    }
}

#[component]
async fn tab_icon(cx: &Cx, tab: Tab) -> Result<impl View> {
    Ok(view! {
        <svg class="rail-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            match tab {
                Tab::Overview => {
                    <rect x="3" y="3" width="7" height="7" rx="1.5"/>
                    <rect x="14" y="3" width="7" height="7" rx="1.5"/>
                    <rect x="3" y="14" width="7" height="7" rx="1.5"/>
                    <rect x="14" y="14" width="7" height="7" rx="1.5"/>
                }
                Tab::Captures => {
                    <path d="M4 8a2 2 0 0 1 2-2h2l1.5-2h5L16 6h2a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8Z"/>
                    <circle cx="12" cy="13" r="3.5"/>
                }
                Tab::Problems => {
                    <path d="M12 3.5 2.8 19.5a1.2 1.2 0 0 0 1.04 1.8h16.32a1.2 1.2 0 0 0 1.04-1.8L12 3.5Z"/>
                    <path d="M12 9.5v4.5"/>
                    <circle cx="12" cy="17" r=".8" fill="currentColor" stroke="none"/>
                }
                Tab::Fixes => {
                    <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.1-3.1a5.5 5.5 0 0 1-7.4 7.4l-6.6 6.6a2 2 0 0 1-2.8-2.8l6.6-6.6a5.5 5.5 0 0 1 7.4-7.4l-3.3 2.9Z"/>
                }
                Tab::Design => {
                    <path d="M12 3a9 9 0 0 0 0 18 1.5 1.5 0 0 0 1.5-1.5c0-.4-.15-.76-.4-1.04-.24-.28-.39-.64-.39-1.04a1.5 1.5 0 0 1 1.5-1.5h1.8A5 5 0 0 0 21 11c0-4.42-4.03-8-9-8Z"/>
                    <circle cx="7.5" cy="11.5" r="1" fill="currentColor" stroke="none"/>
                    <circle cx="10.5" cy="7.5" r="1" fill="currentColor" stroke="none"/>
                    <circle cx="15" cy="8" r="1" fill="currentColor" stroke="none"/>
                }
                Tab::Settings => {
                    <circle cx="12" cy="12" r="3"/>
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"/>
                }
            }
        </svg>
    })
}

#[component]
async fn project_masthead(
    cx: &Cx,
    project: &crate::project::ProjectMeta,
    tab: Tab,
    open_jobs: &[Job],
) -> Result<impl View> {
    let (id, label, base) = (&project.id, &project.label, &project.base);
    let overview = tab == Tab::Overview;
    let page_title = if overview {
        format!("{label} · Rupicola")
    } else {
        format!("{} · {label} · Rupicola", tab.heading())
    };
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
        <header class="page-title project-masthead" data-page-title=(page_title)>
            <nav class="masthead-crumb" aria-label="Breadcrumb">
                <a class="crumb-link" href="/projects">"Projects"</a>
                <span class="crumb-sep" aria-hidden="true">"/"</span>
                if overview {
                    <h1 class="masthead-heading" title=(id)>(label)</h1>
                } else {
                    <a class="crumb-link" href=(base) title=(id)>(label)</a>
                    <span class="crumb-sep" aria-hidden="true">"/"</span>
                    <h1 class="masthead-heading">(tab.heading())</h1>
                }
            </nav>
            <div class="jobs-indicator" role="status" aria-live="polite" data-jobs-url=(jobs_url) if open_jobs.is_empty() { hidden="" }>(jobs_summary)</div>
        </header>
    })
}

/// The project page around one tab: breadcrumbs, heading and tabs; `child` is the tab.
#[component]
pub async fn project_frame(
    cx: &Cx,
    project: &crate::project::ProjectMeta,
    tab: Tab,
    open_jobs: Vec<Job>,
    #[default] partial: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let (id, base, role) = (&project.id, &project.base, project.role);
    Ok(view! {
        project_masthead(project: project, tab: tab, open_jobs: &open_jobs)
        if partial {
            <div class="project-stage" data-project-stage="">
                (child)
            </div>
        } else {
            <div class="project-shell" data-project-id=(id)>
                <nav class="project-rail" aria-label="Project" data-project-rail="">
                    <span class="rail-indicator" aria-hidden="true"></span>
                    for each in Tab::ALL {
                        if each != Tab::Settings || role == Role::Owner {
                            let href = if each == Tab::Overview {
                                base.clone()
                            } else {
                                format!("{base}/{}", each.slug())
                            };
                            <a href=(href) data-nav-partial="" aria-label=(each.label()) title=(each.label()) if tab == each { aria-current="page" }>
                                tab_icon(tab: each)
                                <span class="sr-only">(each.label())</span>
                                <span class="rail-tooltip" aria-hidden="true">(each.label())</span>
                            </a>
                        }
                    }
                </nav>
                <div class="project-stage" data-project-stage="">
                    (child)
                </div>
            </div>
        }
    })
}
