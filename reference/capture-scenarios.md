# Capture bundles & web scenarios

Use a v1 TOML scenario with an existing HTTP(S) preview for before and after runs driven by external or server-managed capture tooling, then publish the resulting v1 bundle (`manifest.json` + media files) to the Rupicola backend. Start the preview server with your project's normal command and use test accounts and fixture data.

## Scenario format & publishing

Run the capture with your Playwright/Chromium runner and publish the directory with the CLI:

```sh
# After external capture produces .rupicola/captures/settings-before/manifest.json:
rupicola publish .rupicola/captures/settings-before --server-url https://rupicola.example.com
```

A scenario with clips typically uses headless Chromium / Playwright steps (`click`, `hover`, `fill`, `press`, `ready`) and `ffmpeg` to produce frames, `analysis.json`, contact sheets, and optional MP4 previews. Checkpoints and clips need distinct IDs with ASCII letters, digits, hyphens or underscores. Keep each clip at 2–60 post-trigger samples spanning at most 10 seconds. Use another profile for dark theme, a different viewport or reduced motion.

A settings preview with a button and a visible popup can use this scenario:

```toml
version = 1
name = "Settings popover"
url = "http://127.0.0.1:4321/settings"
revision = "before-focus-fix" # Optional identifier for the target revision.
timeout_ms = 5000 # Optional per-operation timeout.

[project]
id = "settings-client"
label = "Settings client"

[profile]
width = 800
height = 600
scale = 1.0
locale = "en-US"
color_scheme = "light"
reduced_motion = false

[[steps]]
kind = "ready"
selector = "#theme-options"

[[steps]]
kind = "checkpoint"
id = "closed"

[[steps]]
kind = "clip"
id = "open"
samples = 16
interval_ms = 40
trigger = { kind = "click", selector = "#theme-options" }

[[steps]]
kind = "ready"
selector = "#theme-panel"

[[steps]]
kind = "checkpoint"
id = "opened"
```

In a Git checkout, `rupicola publish DIR` detects the project from Git or uses an explicit `rupicola link`. The `[project]` section in `manifest.json` selects the project recorded in the bundle.

## Inspect a run

Open the checkpoints and clip contact sheets. The clip's `analysis.json` records the pixel-change heuristic, sample indices, scheduled times and observed screenshot start times relative to the trigger. The baseline came before the trigger and uses a zero marker. When a preview exists, each retained frame includes a separate `video_ms` seek offset. Keep the timing difference in mind when judging apparent stalls, jumps or cuts. Screenshot sampling and the constant-frame-rate MP4 preview provide approximate motion evidence. They cannot establish compositor frame pacing.

The bundle retains the baseline, midpoint and last PNG, plus frames near changes or flagged steps where available, along with the analysis, contact sheet and optional MP4.

Compare the same checkpoint across runs in the panel (Before/After view). Record a concrete observation after inspecting the evidence, with the `capture_review` MCP tool or the panel's review form; `evidence_artifact_id` points at one of the capture's artifacts.

Capture status and inspection status are independent. A review appends a finding and marks inspection recorded. After an authorized fix, rerun the same scenario with the updated revision, then inspect and record new observations.

The [Playwright Browser API](https://playwright.dev/docs/api/class-browser) documents the context settings used here.
