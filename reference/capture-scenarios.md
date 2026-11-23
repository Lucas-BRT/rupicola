# Headless web capture

Use a v1 TOML scenario with an existing HTTP(S) preview for before and after runs. Each run launches its own headless Chromium context and writes a v1 manifest. Start the preview server with your project's normal command and use test accounts and fixture data.

## Run a scenario

Install Node.js and Playwright with its Chromium browser. From the project directory, where Playwright is installed:

```sh
npx playwright install chromium
rupicola capture settings.toml --out .rupicola/captures/settings-before --video
```

Supply `--playwright-module /path/to/node_modules/playwright` (or set `RUPICOLA_PLAYWRIGHT_DIR`) when Playwright is installed elsewhere. `--backend` picks what captures the scenario: `playwright` (the default) runs every step, while `chromium` only takes screenshots and so refuses a scenario with actions or clips. A scenario with clips needs `ffmpeg` to cut each recorded clip into its frames; `--video` keeps the recorded clip as an MP4 preview. The output directory must be new. Exit code `0` means capture and requested artifact generation completed. A blocked run exits `1` and includes its blocker and any usable artifacts in the manifest. A scenario rejected before capture exits `1` with a validation message.

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

Actions use Playwright locator selectors and support `click`, `hover`, `fill` (with `value`), `press` (with `key`), and `ready` (wait for a visible element). Checkpoints and clips need distinct IDs with ASCII letters, digits, hyphens or underscores. A clip captures a baseline, performs its trigger, then samples the viewport. Keep each clip at 2–60 post-trigger samples spanning at most 10 seconds. Use another profile for dark theme, a different viewport or reduced motion.

In a Git checkout, omit `--out` to capture into a scratch directory that is removed
after publishing to the backend (`--server-url` or `RUPICOLA_SERVER_URL`).
Rupicola detects the project from Git or uses an explicit `rupicola link`.
The `[project]` section is optional in this mode. With `--out`, supply that
section to choose the project recorded in the manifest.

## Inspect a run

Open the checkpoints and clip contact sheets. The clip's `analysis.json` records the pixel-change heuristic, sample indices, scheduled times and observed screenshot start times relative to the trigger. The baseline came before the trigger and uses a zero marker. When a preview exists, each retained frame includes a separate `video_ms` seek offset. Keep the timing difference in mind when judging apparent stalls, jumps or cuts. Screenshot sampling and the constant-frame-rate MP4 preview provide approximate motion evidence. They cannot establish compositor frame pacing.

The run retains the baseline, midpoint and last PNG, plus frames near changes or flagged steps where available. The manifest points to those frames and the analysis, contact sheet and optional MP4. The run removes the other PNG samples after processing.

Compare the same checkpoint across runs with `rupicola diff` or in the panel. Record a concrete observation after inspecting the evidence, with the `capture_review` MCP tool or the panel's review form; `evidence_artifact_id` points at one of the capture's artifacts.

Capture status and inspection status are independent. A review appends a finding and marks inspection recorded. After an authorized fix, rerun the same scenario with the updated revision, then inspect and record new observations.

The [Playwright Browser API](https://playwright.dev/docs/api/class-browser) documents the context settings used here.
