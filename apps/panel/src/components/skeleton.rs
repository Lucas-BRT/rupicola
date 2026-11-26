use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// Skeleton shown while the dashboard board loads, matching the studio triage layout.
#[component]
pub async fn dashboard_skeleton(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="skel-stack" aria-busy="true" aria-label="Loading dashboard">
            <div class="dash-head">
                <span class="skel skel-heading"></span>
            </div>
            <div class="dash-grid">
                <div class="skel-card">
                    <span class="skel skel-title"></span>
                    for _ in 0..6 {
                        <span class="skel skel-row"></span>
                    }
                </div>
                <div class="dash-rail">
                    <div class="skel-card">
                        <span class="skel skel-title"></span>
                        <span class="skel skel-media"></span>
                    </div>
                    <div class="skel-card">
                        <span class="skel skel-title"></span>
                        <span class="skel skel-row"></span>
                        <span class="skel skel-row"></span>
                        <span class="skel skel-row"></span>
                    </div>
                </div>
            </div>
        </div>
    })
}

/// Skeleton shown while the project catalog loads.
#[component]
pub async fn projects_skeleton(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="skel-stack" aria-busy="true" aria-label="Loading projects">
            <div class="page-title studio-heading">
                <span class="skel skel-heading"></span>
                <span class="skel skel-pill"></span>
            </div>
            <div class="skel-grid">
                for _ in 0..6 {
                    <div class="skel-card">
                        <span class="skel skel-title"></span>
                        <span class="skel skel-sm"></span>
                        <span class="skel skel-row"></span>
                    </div>
                }
            </div>
        </div>
    })
}

/// Skeleton shown while a project overview loads.
#[component]
pub async fn overview_skeleton(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="skel-stack" aria-busy="true" aria-label="Loading project overview">
            <div class="studio-counts">
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
            </div>
            <div class="overview-hero">
                <div class="skel-card"><span class="skel skel-title"></span><span class="skel skel-media"></span></div>
                <div class="skel-card"><span class="skel skel-title"></span><span class="skel skel-sm"></span></div>
            </div>
            <div class="skel-card">
                <span class="skel skel-title"></span>
                <span class="skel skel-row"></span>
                <span class="skel skel-row"></span>
                <span class="skel skel-row"></span>
                <span class="skel skel-row"></span>
            </div>
        </div>
    })
}

/// Skeleton matching the captures tab (3 KPI cards + toolbar + run list) to avoid hydration shift.
#[component]
pub async fn captures_tab_skeleton(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="skel-stack" aria-busy="true" aria-label="Loading captures">
            <div class="studio-counts">
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
                <div class="skel-card"><span class="skel skel-sm"></span><span class="skel skel-lg"></span></div>
            </div>
            <div class="propose-toolbar">
                <span class="skel skel-pill"></span>
                <span class="skel skel-pill"></span>
            </div>
            <div class="skel-card">
                <span class="skel skel-title"></span>
                <span class="skel skel-row"></span>
                <span class="skel skel-row"></span>
                <span class="skel skel-row"></span>
            </div>
        </div>
    })
}

/// Shared skeleton for filterable project tabs (problems, fixes, design).
#[component]
pub async fn list_tab_skeleton(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="skel-stack" aria-busy="true" aria-label="Loading">
            <div class="skel-card"><span class="skel skel-row"></span></div>
            for _ in 0..4 {
                <div class="skel-card">
                    <span class="skel skel-title"></span>
                    <span class="skel skel-row"></span>
                </div>
            }
        </div>
    })
}
