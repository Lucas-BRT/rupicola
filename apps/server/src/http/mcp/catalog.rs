use crate::http::ops::MAX_READ_BYTES;
use serde_json::{Value, json};

/// One tool's catalog entry.
pub(super) struct Tool {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) required: &'static [&'static str],
    properties: fn() -> Value,
}

impl Tool {
    pub(super) fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": {
                "type": "object",
                "required": self.required,
                "properties": (self.properties)(),
            },
        })
    }
}

fn text() -> Value {
    json!({"type": "string"})
}

fn string_list() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn severity() -> Value {
    json!({"enum": ["critical", "major", "minor", "polish"]})
}

fn comparisons() -> Value {
    json!({"type": "array", "items": {"type": "object",
        "required": ["label", "before", "after"],
        "properties": {"label": text(), "route": text(), "viewport": text(), "theme": text(),
            "before": text(), "after": text(), "diff": text(),
            "changed_ratio": {"type": "number"},
            "view": {"enum": ["isolated", "integrated"],
                "description": "isolated: the base against the base plus this fix alone. integrated: the base with every approved fix against the same plus this fix."},
            "base_revision": text(), "revision": text(),
            "applied_fixes": {"type": "array", "items": text(),
                "description": "Ids of the approved fixes the before tree already had; empty when isolated."}}}})
}

fn step_properties() -> Value {
    json!({"title": text(), "detail": text(), "verification": text(), "evidence": text()})
}

fn selections_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["label", "items"],
            "properties": {
                "label": text(),
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["capture", "mark"],
                        "properties": {
                            "capture": text(),
                            "artifact": text(),
                            "path": text(),
                            "mark": {"enum": ["proof", "context"]},
                            "fragment": {"type": "object"}
                        }
                    }
                }
            }
        }
    })
}

/// Every tool, in the exact order of `rupicola_api::WORKSPACE_OP_NAMES`.
pub(super) const TOOLS: &[Tool] = &[
    // Identity & projects
    Tool {
        name: "me",
        description: "Show the identity and permissions of the authenticated credential",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "project_list",
        description: "List the projects this credential can see",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "project_get",
        description: "Read one project's metadata",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "project_create",
        description: "Create a project with a stable slug id and a label",
        required: &["id", "label"],
        properties: || json!({"id": text(), "label": text()}),
    },
    Tool {
        name: "project_update",
        description: "Rename a project's label",
        required: &["id", "label"],
        properties: || json!({"id": text(), "label": text()}),
    },
    Tool {
        name: "project_delete",
        description: "Permanently delete a project",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "project_limits",
        description: "Read the effective upload and capture limits for a project",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_settings_get",
        description: "Read a project's capture settings (auto_recapture, base_ref)",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_settings_update",
        description: "Update a project's capture settings (auto_recapture, base_ref)",
        required: &["project"],
        properties: || json!({"project": text(), "auto_recapture": {"type": ["boolean", "null"]}, "base_ref": {"type": ["string", "null"]}}),
    },
    // Captures & artifacts
    Tool {
        name: "capture_list",
        description: "List a project's captures, newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_search",
        description: "Search a project's captures with query parameters (e.g. include=details&limit=20&scenario=home)",
        required: &["project"],
        properties: || json!({"project": text(), "query": text()}),
    },
    Tool {
        name: "capture_get",
        description: "One capture with its checkpoints, clips and review notes",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_capabilities",
        description: "Probe which host OS capture backends are available and return install hints for missing ones",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "capture_publish",
        description: "Publish a capture and its PNG/MP4 media blobs directly through MCP, returning stored artifact IDs for problem_create and fix_create. Pass `data_base64` per member (or `file_path` / `bundle_dir` when connected via `rupicola mcp-bridge`).",
        required: &["project", "scenario", "members"],
        properties: || {
            json!({
                "project": text(),
                "scenario": text(),
                "revision": text(),
                "viewport": text(),
                "theme": text(),
                "bundle_dir": text(),
                "members": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": text(),
                            "role": {"enum": ["checkpoint", "clip-video", "clip-frame", "clip-sheet", "clip-analysis", "ui-snapshot"]},
                            "mime": text(),
                            "path": text(),
                            "data_base64": text(),
                            "file_path": text(),
                            "sha256": text(),
                            "bytes": {"type": "integer", "minimum": 1}
                        }
                    }
                }
            })
        },
    },
    Tool {
        name: "capture_review",
        description: "Leave a review note on a capture, optionally pointing at one of its artifacts",
        required: &["id", "note"],
        properties: || json!({"id": text(), "note": text(), "evidence_artifact_id": text()}),
    },
    Tool {
        name: "capture_citations",
        description: "List what cites a capture, its children, and whether removal is allowed (with confirm token when cited)",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_delete",
        description: "Remove a capture, leaving a tombstone; pass `confirm` from capture_citations when cited",
        required: &["id", "reason"],
        properties: || json!({"id": text(), "reason": text(), "confirm": text()}),
    },
    Tool {
        name: "capture_lineage",
        description: "Read the lineage graph around a capture up to `depth` steps",
        required: &["id"],
        properties: || json!({"id": text(), "depth": {"type": "integer", "minimum": 1}}),
    },
    Tool {
        name: "capture_parents_get",
        description: "Read a capture's declared parents, generation, and edit history",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_parents_edit",
        description: "Replace a capture's parents using optimistic concurrency (`generation`)",
        required: &["id", "generation", "parents", "reason"],
        properties: || {
            json!({"id": text(), "generation": {"type": "integer"}, "reason": text(),
            "parents": {"type": "array", "items": {"type": "object", "required": ["id", "kind"],
                "properties": {"id": text(), "kind": {"enum": ["baseline", "input"]}}}}})
        },
    },
    Tool {
        name: "capture_compare",
        description: "Compare two captures of the same project to see added, removed, and changed members",
        required: &["before", "after"],
        properties: || json!({"before": text(), "after": text()}),
    },
    Tool {
        name: "artifact_read",
        description: "Read a slice of a captured file as base64; page with offset and length",
        required: &["id"],
        properties: || json!({"id": text(), "offset": {"type": "integer", "minimum": 0}, "length": {"type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES}}),
    },
    // Problems
    Tool {
        name: "problem_list",
        description: "List a project's problems, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "problem_list_detailed",
        description: "List every problem of a project with full rules, evidence, selections and comments",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "problem_get",
        description: "One problem with its rules, evidence and comments",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "problem_create",
        description: "File a problem; requires at least one evidence artifact id from the project's captures",
        required: &["project", "title", "severity", "origin", "evidence"],
        properties: || {
            json!({"project": text(), "title": text(), "severity": severity(),
        "origin": {"enum": ["agent-measured", "agent-visual", "user-reported"]},
        "route": text(), "viewport": text(), "theme": text(), "diagnosis": text(), "proposed_solution": text(),
        "rules": {"type": "array", "items": {"type": "object", "required": ["scope", "code", "title"], "properties": {"scope": text(), "code": text(), "title": text()}}},
        "evidence": {"type": "array", "minItems": 1, "items": text()}, "selections": selections_schema()})
        },
    },
    Tool {
        name: "problem_delete",
        description: "Permanently delete a problem from its project",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "problem_triage",
        description: "Change a problem's status, severity or proposed solution",
        required: &["id"],
        properties: || {
            json!({"id": text(),
        "status": {"enum": ["open", "in-progress", "intentional", "false-positive", "needs-reprocess", "resolved"]},
        "severity": severity(), "proposed_solution": text()})
        },
    },
    Tool {
        name: "problem_comment",
        description: "Add a comment to a problem",
        required: &["id", "text"],
        properties: || json!({"id": text(), "text": text()}),
    },
    Tool {
        name: "problem_analyze",
        description: "Infer and file deduplicated, unsuppressed problems from a capture's findings",
        required: &["project", "capture"],
        properties: || json!({"project": text(), "capture": text()}),
    },
    Tool {
        name: "problem_cite_selection",
        description: "Attach an existing selection to a problem",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    Tool {
        name: "problem_uncite_selection",
        description: "Detach a selection from a problem",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    // Fixes
    Tool {
        name: "fix_list",
        description: "List a project's fixes, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "fix_list_detailed",
        description: "List every fix of a project with comparisons, selections, steps and reviews",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "fix_get",
        description: "One fix with its comparisons, steps and reviews",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "fix_create",
        description: "File a fix for one or more problems; they go in progress until the fix is approved",
        required: &["project", "title", "problems"],
        properties: || {
            json!({"project": text(), "title": text(), "summary": text(), "git_ref": text(),
        "problems": string_list(), "tactics": string_list(), "files": string_list(),
        "comparisons": comparisons(), "baseline": selections_schema(), "proof": selections_schema(),
        "steps": {"type": "array", "items": {"type": "object", "required": ["title"], "properties": step_properties()}}})
        },
    },
    Tool {
        name: "fix_step",
        description: "Record one more verified step of a fix",
        required: &["id", "title"],
        properties: || {
            let mut properties = step_properties();
            properties["id"] = text();
            properties
        },
    },
    Tool {
        name: "fix_review",
        description: "Approve a fix (resolves its problems), request changes, or mark it superseded",
        required: &["id", "outcome"],
        properties: || {
            json!({"id": text(),
        "outcome": {"enum": ["approved", "changes-requested", "superseded"]}, "comment": text()})
        },
    },
    Tool {
        name: "fix_cite_selection",
        description: "Attach an existing selection to a fix as baseline or proof",
        required: &["id", "selection", "role"],
        properties: || json!({"id": text(), "selection": text(), "role": {"enum": ["baseline", "proof"]}}),
    },
    Tool {
        name: "fix_uncite_selection",
        description: "Detach a selection from a fix",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    // Design & design systems
    Tool {
        name: "design_list",
        description: "List a project's design records (briefs, directions, tactics, decisions), newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_record",
        description: "Record a design brief, direction, tactic or decision; supersedes names the record it replaces",
        required: &["project", "kind", "status", "title"],
        properties: || {
            json!({"project": text(),
        "kind": {"enum": ["brief", "direction", "tactic", "decision"]},
        "status": {"enum": ["proposed", "adopted", "rejected"]},
        "title": text(), "note": text(), "surface": text(), "supersedes": text(),
        "reference": text(), "evidence": text()})
        },
    },
    Tool {
        name: "design_system_list",
        description: "List a project's design-system versions and which one is adopted",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_system_get",
        description: "Read one design-system version document (pass `adopted` or a version id)",
        required: &["project", "version"],
        properties: || json!({"project": text(), "version": text()}),
    },
    Tool {
        name: "design_system_create",
        description: "Create a new draft design-system version starting from the adopted version",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_system_edit",
        description: "Replace the document of a draft design-system version with optimistic concurrency (`generation`)",
        required: &["project", "version", "generation", "document"],
        properties: || json!({"project": text(), "version": text(), "generation": {"type": "integer"}, "document": {"type": "object"}}),
    },
    Tool {
        name: "design_system_act",
        description: "Validate, review, adopt, or vary a design-system version",
        required: &["project", "version", "action", "generation"],
        properties: || {
            json!({"project": text(), "version": text(),
            "action": {"enum": ["validate", "review", "adopt", "vary"]},
            "generation": {"type": "integer"}, "target": text(), "note": text()})
        },
    },
    // Jobs (including runner lifecycle)
    Tool {
        name: "job_list",
        description: "List a project's capture jobs, newest first; open=true keeps the ones waiting or running",
        required: &["project"],
        properties: || json!({"project": text(), "open": {"type": "boolean"}}),
    },
    Tool {
        name: "job_get",
        description: "One capture job: status, cells_done of cells_total, the runner, and the captures it made",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "job_create",
        description: "Queue a capture job: run a scenario on one or more trees of the code. A runner takes it, builds each tree (a git ref with other refs merged on top), captures the scenario's matrix and uploads the captures. Poll job_get for cells_done of cells_total",
        required: &["project", "scenario", "trees"],
        properties: || {
            json!({"project": text(), "scenario": text(),
                "hash": {"type": "string", "description": "Exact scenario version; the latest when absent."},
                "fix": {"type": "string", "description": "The fix the captures are for."},
                "trees": {"type": "array", "items": {"type": "object", "required": ["name", "base"],
                    "properties": {"name": text(), "base": {"type": "string", "description": "Git ref to check out."},
                        "merge": string_list(), "fixes": string_list()}}}})
        },
    },
    Tool {
        name: "job_claim",
        description: "Claim the oldest queued capture job for a project (returns null when queue is empty)",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "job_progress",
        description: "Report runner progress on a claimed job and renew its lease",
        required: &["id", "cells_done"],
        properties: || json!({"id": text(), "cells_done": {"type": "integer", "minimum": 0}}),
    },
    Tool {
        name: "job_complete",
        description: "Finish a claimed job with the captures it produced",
        required: &["id", "captures"],
        properties: || json!({"id": text(), "captures": {"type": "array", "items": {"type": "object"}}}),
    },
    Tool {
        name: "job_fail",
        description: "Mark a claimed job as failed with a reason",
        required: &["id", "error"],
        properties: || json!({"id": text(), "error": text()}),
    },
    Tool {
        name: "job_cancel",
        description: "Cancel a capture job that has not finished; a runner holding it stops at its next report",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    // Scenarios
    Tool {
        name: "scenario_list",
        description: "List a project's scenarios, the capture definitions its repository owns, latest version of each",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "scenario_get",
        description: "One scenario definition: its latest version, or the exact version with `hash`",
        required: &["project", "name"],
        properties: || json!({"project": text(), "name": text(), "hash": text()}),
    },
    Tool {
        name: "scenario_put",
        description: "Push a scenario definition. The repository's TOML file is the source of truth; this keeps a copy under the hash of the parsed definition. Same definition twice is a no-op",
        required: &["project", "scenario"],
        properties: || {
            json!({"project": text(),
                "scenario": {"type": "object", "required": ["name", "url"],
                    "description": "The parsed .rupicola/scenarios/*.toml: name, url, environment, matrix {viewports, themes}, themes, steps, checks.",
                    "properties": {"name": text(), "url": text(), "environment": {"type": "object"},
                        "matrix": {"type": "object"}, "themes": {"type": "object"},
                        "steps": {"type": "array"}, "checks": {"type": "object"}}}})
        },
    },
    // Selections
    Tool {
        name: "selection_create",
        description: "Create a standalone selection of capture artifacts/fragments in a project",
        required: &["project", "label", "items"],
        properties: || {
            json!({"project": text(), "label": text(),
            "items": {"type": "array", "items": {"type": "object", "required": ["capture", "mark"],
                "properties": {"capture": text(), "artifact": text(), "path": text(),
                    "mark": {"enum": ["proof", "context"]}, "fragment": {"type": "object"}}}}})
        },
    },
    Tool {
        name: "selection_get",
        description: "Read one selection by id",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    // Topics
    Tool {
        name: "topic_tree",
        description: "Read a project's hierarchical topic tree",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "topic_create",
        description: "Create a topic path (like mkdir -p) in a project",
        required: &["project", "path"],
        properties: || json!({"project": text(), "path": text()}),
    },
    Tool {
        name: "topic_rename",
        description: "Rename a topic with optimistic concurrency (`generation`)",
        required: &["id", "name", "generation"],
        properties: || json!({"id": text(), "name": text(), "generation": {"type": "integer"}}),
    },
    Tool {
        name: "topic_move",
        description: "Move a topic under a new parent (or null for root) with optimistic concurrency (`generation`)",
        required: &["id", "generation"],
        properties: || json!({"id": text(), "parent": {"type": ["string", "null"]}, "generation": {"type": "integer"}}),
    },
    Tool {
        name: "topic_delete",
        description: "Delete a topic; pass recursive=true to delete subtopics as well",
        required: &["id"],
        properties: || json!({"id": text(), "recursive": {"type": "boolean"}}),
    },
    Tool {
        name: "topic_captures",
        description: "List captures filed under a topic (direct=true excludes subtopics)",
        required: &["id"],
        properties: || json!({"id": text(), "direct": {"type": "boolean"}}),
    },
    Tool {
        name: "topic_file_capture",
        description: "File a capture under a topic",
        required: &["id", "capture"],
        properties: || json!({"id": text(), "capture": text()}),
    },
    Tool {
        name: "topic_unfile_capture",
        description: "Remove a capture from a topic",
        required: &["id", "capture"],
        properties: || json!({"id": text(), "capture": text()}),
    },
    // Offline static analysis (rupicola-core; executed locally by rupicola mcp-bridge)
    Tool {
        name: "offline_discover",
        description: "Discover Rust UI framework dependencies in a local workspace without building",
        required: &[],
        properties: || json!({"path": text()}),
    },
    Tool {
        name: "offline_scan",
        description: "Run static Rust UI syntax scan against a local path",
        required: &[],
        properties: || json!({"path": text(), "framework": text(), "include_tests": {"type": "boolean"}}),
    },
    Tool {
        name: "offline_check",
        description: "Validate skill catalog Markdown files and links at a local path",
        required: &[],
        properties: || json!({"path": text()}),
    },
    Tool {
        name: "offline_rules",
        description: "List the supported static UI scan rules",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "offline_modules",
        description: "List available framework and UX reference module paths",
        required: &[],
        properties: || json!({}),
    },
    // Capture execution (via rupicola-capture)
    Tool {
        name: "capture_run_scenario",
        description: "Execute a stored project scenario via rupicola-capture (Chromium CDP) and publish the capture",
        required: &["project", "scenario", "base_url"],
        properties: || json!({"project": text(), "scenario": text(), "base_url": text(), "viewport": text(), "theme": text()}),
    },
    Tool {
        name: "capture_record_clip",
        description: "Record a video clip of a URL via rupicola-capture (Chromium CDP) and publish the capture",
        required: &["project", "url", "fps", "duration_ms"],
        properties: || {
            json!({"project": text(), "url": text(), "fps": {"type": "integer", "minimum": 1, "maximum": 60},
            "duration_ms": {"type": "integer", "minimum": 50}, "viewport": text(), "theme": text()})
        },
    },
    Tool {
        name: "capture_snapshot_checkpoint",
        description: "Capture a single checkpoint PNG (+ optional UI snapshot) via rupicola-capture (Chromium CDP)",
        required: &["project", "url", "label"],
        properties: || {
            json!({"project": text(), "url": text(), "label": text(),
            "include_ui_snapshot": {"type": "boolean"}, "viewport": text(), "theme": text()})
        },
    },
    // Progressive token-efficient video/media inspection
    Tool {
        name: "media_probe",
        description: "Probe image or video metadata (dimensions, duration, fps, frame_count, codec) with zero images in context",
        required: &["artifact_id"],
        properties: || json!({"artifact_id": text()}),
    },
    Tool {
        name: "media_motion_diff",
        description: "Detect frame-to-frame pixel/luminance spikes (flashes, flickers, jumps) numerically as pure JSON before opening PNGs",
        required: &["artifact_id"],
        properties: || json!({"artifact_id": text(), "threshold": {"type": "number", "minimum": 0.0, "maximum": 1.0}}),
    },
    Tool {
        name: "media_contact_sheet",
        description: "Render one cols x rows contact-sheet PNG for single-image visual triage of a video clip",
        required: &["artifact_id", "cols", "rows"],
        properties: || {
            json!({"artifact_id": text(), "cols": {"type": "integer", "minimum": 1, "maximum": 10},
            "rows": {"type": "integer", "minimum": 1, "maximum": 10},
            "start_ms": {"type": "integer", "minimum": 0}, "end_ms": {"type": "integer", "minimum": 1}})
        },
    },
    Tool {
        name: "media_sample_frames",
        description: "Progressively extract video frames (strategy: coarse | bisect | window) without re-reading already-inspected frames",
        required: &["artifact_id", "strategy"],
        properties: || {
            json!({"artifact_id": text(),
            "strategy": {"enum": ["coarse", "bisect", "window"]},
            "count": {"type": "integer", "minimum": 1, "maximum": 60},
            "start_ms": {"type": "integer", "minimum": 0},
            "end_ms": {"type": "integer", "minimum": 1},
            "fps": {"type": "integer", "minimum": 1, "maximum": 60},
            "exclude_frames": {"type": "array", "items": {"type": "integer", "minimum": 0}}})
        },
    },
    // Visual diff & targeted crop
    Tool {
        name: "visual_diff_images",
        description: "Compare two PNG artifacts (highlight | heatmap | side_by_side), store the diff artifact, and return changed_ratio + changed_regions",
        required: &["before_artifact_id", "after_artifact_id"],
        properties: || {
            json!({"before_artifact_id": text(), "after_artifact_id": text(),
            "mode": {"enum": ["highlight", "heatmap", "side_by_side"]}})
        },
    },
    Tool {
        name: "visual_crop_region",
        description: "Crop a rectangular region from a PNG artifact to inspect details without spending tokens on the full screen",
        required: &["artifact_id", "x", "y", "width", "height"],
        properties: || {
            json!({"artifact_id": text(),
            "x": {"type": "integer", "minimum": 0}, "y": {"type": "integer", "minimum": 0},
            "width": {"type": "integer", "minimum": 1}, "height": {"type": "integer", "minimum": 1}})
        },
    },
];

/// What `tools/list` returns.
pub(super) fn catalog() -> Vec<Value> {
    TOOLS.iter().map(Tool::describe).collect()
}
