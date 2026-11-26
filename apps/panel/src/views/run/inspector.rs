use super::Capture;
use crate::components::form::stack_form;
use crate::{Account, media};
use rupicola_api::{
    Capture as ApiCapture, CaptureDiff, CaptureSummary, Checkpoint, Citations, Clip, Finding,
    LineageGraph, MotionDiffReport, ProblemSummary, Topic,
};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

#[component]
pub(super) async fn inspector_rail(cx: &Cx, data: &Capture) -> Result<impl View> {
    let Capture {
        record: run,
        candidates,
        requested,
        before_artifact_ids,
        may_review,
        width,
        height,
        form_url,
        citations,
        lineage,
        diff,
        parity_mismatches,
        diff_artifact_id,
        motion_report,
        topics,
        open_problems,
        ..
    } = data;
    let account = context::<Account>(cx);
    Ok(view! {
        <aside class="canvas-rail" aria-label="Inspection and comparison controls">
            summary(run: run, width: width, height: height)
            if !candidates.is_empty() {
                comparison_control(form_url: form_url, candidates: candidates, requested: requested.as_deref())
            }
            if let Some(diff) = diff {
                capture_diff_section(
                    form_url: form_url,
                    token: &account.csrf,
                    diff: diff,
                    parity_mismatches: parity_mismatches.as_deref(),
                    checkpoints: &run.checkpoints,
                    before_artifact_ids: before_artifact_ids,
                    requested_before: requested.as_deref().unwrap_or(""),
                    diff_artifact_id: diff_artifact_id.as_deref(),
                )
            }
            if !run.clips.is_empty() {
                motion_spikes_section(form_url: form_url, clips: &run.clips, motion_report: motion_report.as_ref())
            }
            if let Some(graph) = lineage
                && graph.nodes.len() > 1
            {
                lineage_section(graph: graph)
            }
            if !run.findings.is_empty() {
                findings(items: &run.findings)
            }
            if *may_review {
                if run.plan_spec.is_some() {
                    <section class="surface review" id="recapture">
                        <h2>"Deterministic recapture"</h2>
                        stack_form(action: form_url, token: &account.csrf,
                            <input type="hidden" name="action" value="recapture">
                            <button type="submit" class="button">"Recapture with exact plan"</button>
                        )
                    </section>
                }
                review_form(form_url: form_url, token: &account.csrf, checkpoints: &run.checkpoints)
                organize_section(
                    form_url: form_url,
                    token: &account.csrf,
                    checkpoints: &run.checkpoints,
                    topics: topics,
                    problems: open_problems,
                )
            }
            if let Some(cit) = citations {
                delete_capture_section(form_url: form_url, token: &account.csrf, citations: cit)
            }
        </aside>
    })
}

/// The capture's state, environment and revision.
#[component]
async fn summary(cx: &Cx, run: &ApiCapture, width: &str, height: &str) -> Result<impl View> {
    let scheme = run.environment.color_scheme.as_deref().unwrap_or("");
    Ok(view! {
        <p class="metadata run-metadata"><span class="status">(run.status.as_str())</span> <span>"Inspection " (run.inspection.as_str())</span> if let Some(renderer) = &run.environment.renderer { <code>(renderer)</code> }</p>
        <p class="muted">
            (format!("{} × {}", width, height))
            if !scheme.is_empty() { " · " (scheme) }
            if run.environment.reduced_motion { " · Reduced motion" }
        </p>
        if let Some(revision) = &run.revision { <p>"Revision " <code>(revision)</code></p> }
        if let Some(blocker) = &run.blocker { <p class="blocker">(blocker)</p> }
    })
}

/// The choice of an earlier capture of the same scenario to compare with.
#[component]
async fn comparison_control(
    cx: &Cx,
    form_url: &str,
    candidates: &[CaptureSummary],
    requested: Option<&str>,
) -> Result<impl View> {
    Ok(view! {
        <form class="filters comparison-control" method="get" action=(form_url)>
            <label for="before">"Compare with"</label>
            <select id="before" name="before">
                <option value="">"Current run only"</option>
                for candidate in candidates {
                    <option value=(&candidate.id) data-epoch-ms=(candidate.created_at.to_string())
                        data-time-suffix=(candidate.id.chars().take(16).collect::<String>())
                        if requested == Some(candidate.id.as_str()) { selected="" }>
                        (&candidate.id)
                    </option>
                }
            </select>
            <button class="button" type="submit">"Apply"</button>
        </form>
    })
}

/// The review notes left on the capture.
#[component]
async fn findings(cx: &Cx, items: &[Finding]) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <section>
            <h2>"Findings"</h2>
            <ul>
                for finding in items {
                    <li>
                        (&finding.note)
                        if let Some(evidence) = &finding.evidence_artifact_id {
                            " " <a href=(url(evidence))>"Evidence"</a>
                        }
                    </li>
                }
            </ul>
        </section>
    })
}

/// The form that records an inspection note, optionally pointing at a screenshot.
#[component]
async fn review_form(
    cx: &Cx,
    form_url: &str,
    token: &str,
    checkpoints: &[Checkpoint],
) -> Result<impl View> {
    Ok(view! {
        <section class="surface review" id="review">
            <h2>"Record inspection"</h2>
            stack_form(action: form_url, token: token,
                <label for="note">"Observation"</label>
                <textarea id="note" name="note" required="" maxlength="2000" rows="3"></textarea>
                if !checkpoints.is_empty() {
                    <label for="evidence">"Evidence"</label>
                    <select id="evidence" name="evidence">
                        <option value="">"No checkpoint"</option>
                        for checkpoint in checkpoints {
                            <option value=(&checkpoint.artifact.id)>(&checkpoint.id)</option>
                        }
                    </select>
                }
                <button type="submit" class="button primary">"Save inspection"</button>
            )
        </section>
    })
}

/// Danger zone for removing a capture, listing any citations and carrying the confirmation token.
#[component]
async fn delete_capture_section(
    cx: &Cx,
    form_url: &str,
    token: &str,
    citations: &Citations,
) -> Result<impl View> {
    Ok(view! {
        <section class="surface review danger-zone" id="delete-capture">
            <details class="triage-note-drawer">
                <summary>"Delete capture…"</summary>
                if !citations.citations.is_empty() {
                    <p class="notice">
                        (format!("Cited by {} record(s). Removing leaves a tombstone and reopens uncited problems.", citations.citations.len()))
                    </p>
                    <ul class="audit">
                        for c in &citations.citations {
                            <li><strong class="mono">(c.kind.as_str())</strong> " · " <span class="mono">(&c.id)</span></li>
                        }
                    </ul>
                }
                if !citations.allowed {
                    <p class="muted">"Deletion is blocked by project or server policy for this capture."</p>
                } else {
                    stack_form(action: form_url, token: token,
                        <input type="hidden" name="action" value="delete">
                        if let Some(confirm) = &citations.confirm {
                            <input type="hidden" name="confirm" value=(confirm)>
                        }
                        <label for="delete-reason">"Reason for removal"</label>
                        <input id="delete-reason" name="reason" type="text" required="" maxlength="500" placeholder="Superseded or captured with wrong env">
                        <button type="submit" class="button button-danger">
                            if citations.confirm.is_some() { "Confirm & delete cited capture" } else { "Delete capture" }
                        </button>
                    )
                }
            </details>
        </section>
    })
}

/// Lineage graph links (parents & children) around this capture.
#[component]
async fn lineage_section(cx: &Cx, graph: &LineageGraph) -> Result<impl View> {
    Ok(view! {
        <section class="surface review" id="lineage">
            <h2>"Lineage"</h2>
            <ul class="audit">
                for node in &graph.nodes {
                    if node.id != graph.root {
                        <li>
                            <a href=(format!("/run/{}", node.id)) class="mono">(&node.scenario) " · " (node.id.chars().take(12).collect::<String>())</a>
                            if node.deleted { <span class="muted">" (removed)"</span> }
                        </li>
                    }
                }
            </ul>
        </section>
    })
}

/// File-level diff summary and on-demand visual diff renderer when comparing two captures.
#[component]
#[allow(clippy::too_many_arguments)]
async fn capture_diff_section(
    cx: &Cx,
    form_url: &str,
    token: &str,
    diff: &CaptureDiff,
    parity_mismatches: Option<&[rupicola_api::PlanMismatch]>,
    checkpoints: &[Checkpoint],
    before_artifact_ids: &BTreeMap<String, String>,
    requested_before: &str,
    diff_artifact_id: Option<&str>,
) -> Result<impl View> {
    let first_pair = checkpoints.iter().find_map(|cp| {
        before_artifact_ids
            .get(cp.id.as_str())
            .map(|before_id| (before_id.clone(), cp.artifact.id.clone()))
    });
    Ok(view! {
        <section class="surface review" id="visual-diff">
            <h2>"Capture diff"</h2>
            if let Some(mismatches) = parity_mismatches {
                if mismatches.is_empty() {
                    <p class="status">"Parity verified ✓ (identical route, viewport, DPR, zoom, theme & steps)"</p>
                } else {
                    <div class="notice" role="alert">
                        <strong>"Plan mismatch ⚠"</strong>
                        for m in mismatches {
                            <p class="mono">(format!("{}: {} → {}", m.field, m.before, m.after))</p>
                        }
                    </div>
                }
            }
            <p class="muted">
                (format!("{} changed · {} added · {} removed · {} same", diff.changed.len(), diff.added.len(), diff.removed.len(), diff.same.len()))
            </p>
            if let Some((before_art, after_art)) = first_pair {
                stack_form(action: form_url, token: token,
                    <input type="hidden" name="action" value="visual-diff">
                    <input type="hidden" name="before_capture_id" value=(requested_before)>
                    <input type="hidden" name="before_artifact_id" value=(before_art)>
                    <input type="hidden" name="after_artifact_id" value=(after_art)>
                    <label for="diff-mode">"Visual diff mode"</label>
                    <select id="diff-mode" name="mode">
                        <option value="highlight">"Highlight"</option>
                        <option value="heatmap">"Heatmap"</option>
                        <option value="side_by_side">"Side by side"</option>
                    </select>
                    <button type="submit" class="button">"Generate visual diff"</button>
                )
            }
            if let Some(art_id) = diff_artifact_id {
                <figure>
                    <a href=(media::url(art_id))>
                        <img src=(media::url(art_id)) alt="Generated visual diff" loading="lazy">
                    </a>
                    <figcaption>"Rendered visual diff"</figcaption>
                </figure>
            }
        </section>
    })
}

/// On-demand numeric motion/flicker spike detection for recorded clips.
#[component]
async fn motion_spikes_section(
    cx: &Cx,
    form_url: &str,
    clips: &[Clip],
    motion_report: Option<&MotionDiffReport>,
) -> Result<impl View> {
    Ok(view! {
        <section class="surface review" id="motion-spikes">
            <h2>"Motion spikes"</h2>
            for clip in clips {
                if let Some(video) = &clip.video {
                    <a class="button" href=(format!("{form_url}?motion={}#motion-spikes", video.id))>
                        (format!("Detect spikes · {}", clip.id))
                    </a>
                }
            }
            if let Some(report) = motion_report {
                if report.spikes.is_empty() {
                    <p class="muted">"No motion or flicker spikes above threshold."</p>
                } else {
                    <ul class="audit">
                        for spike in &report.spikes {
                            <li>
                                <button type="button" class="button" data-seek=(format!("{}.{:03}", spike.timestamp_ms / 1000, spike.timestamp_ms % 1000))>
                                    (format!("F{} · {} ms · Δluma {:+.2}", spike.frame_index, spike.timestamp_ms, spike.luma_delta))
                                </button>
                            </li>
                        }
                    </ul>
                }
            }
        </section>
    })
}

/// File capture under a topic or create a standalone selection (optionally cited on a problem).
#[component]
async fn organize_section(
    cx: &Cx,
    form_url: &str,
    token: &str,
    checkpoints: &[Checkpoint],
    topics: &[Topic],
    problems: &[ProblemSummary],
) -> Result<impl View> {
    Ok(view! {
        <section class="surface review" id="organize">
            <h2>"Topics & selections"</h2>
            if !topics.is_empty() {
                stack_form(action: form_url, token: token,
                    <input type="hidden" name="action" value="file-topic">
                    <label for="topic-id">"File under topic"</label>
                    <select id="topic-id" name="topic_id">
                        for t in topics {
                            <option value=(&t.id)>(&t.path)</option>
                        }
                    </select>
                    <button type="submit" class="button">"File in topic"</button>
                )
                if !checkpoints.is_empty() {
                    <hr>
                }
            }
            if !checkpoints.is_empty() {
                stack_form(action: form_url, token: token,
                    <input type="hidden" name="action" value="create-selection">
                    <label for="sel-label">"New selection label"</label>
                    <input id="sel-label" name="label" type="text" required="" maxlength="120" placeholder="Header contrast proof">
                    <label for="sel-art">"Artifact"</label>
                    <select id="sel-art" name="artifact_id">
                        for cp in checkpoints {
                            <option value=(&cp.artifact.id)>(&cp.id)</option>
                        }
                    </select>
                    if !problems.is_empty() {
                        <label for="sel-prob">"Attach to problem (optional)"</label>
                        <select id="sel-prob" name="problem_id">
                            <option value="">"Standalone selection"</option>
                            for p in problems {
                                <option value=(&p.id)>(&p.title)</option>
                            }
                        </select>
                    }
                    <button type="submit" class="button">"Create selection"</button>
                )
            }
        </section>
    })
}
