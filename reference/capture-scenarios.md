# Headless web capture

Use a v1 TOML scenario with an existing HTTP(S) preview for before and after runs. Each run launches its own headless Chromium context and writes a v1 manifest. Start the preview server with your project's normal command and use test accounts and fixture data.

## Run a scenario

Install Node.js and Playwright with its Chromium browser. From the project directory, where Playwright is installed:

```sh
npx playwright install chromium
rupicola capture settings.toml --out .rupicola/captures/settings-before --video
```

Supply `--playwright-module /path/to/node_modules/playwright` when Playwright is installed elsewhere. Without `--video`, the run needs only Node.js and Playwright. MP4 previews additionally require an `ffmpeg` build with `libx264`. The output directory must be new. Exit code `0` means capture and requested artifact generation completed. A blocked run exits `1` and includes its blocker and any usable artifacts in the manifest. A scenario rejected before capture exits `1` with a validation message.

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

In a Git checkout, omit `--out` to write directly to the local panel directory.
Rupicola detects the project from Git or uses an explicit `rupicola link`.
The `[project]` section is optional in this mode. With `--out`, supply that
section to choose the project recorded in the manifest.

## Inspect a run

Open the checkpoints and clip contact sheets. The clip's `analysis.json` records the pixel-change heuristic, sample indices, scheduled times and observed screenshot start times relative to the trigger. The baseline came before the trigger and uses a zero marker. When a preview exists, each retained frame includes a separate `video_ms` seek offset. Keep the timing difference in mind when judging apparent stalls, jumps or cuts. Screenshot sampling and the constant-frame-rate MP4 preview provide approximate motion evidence. They cannot establish compositor frame pacing.

The run retains the baseline, midpoint and last PNG, plus frames near changes or flagged steps where available. The manifest points to those frames and the analysis, contact sheet and optional MP4. The run removes the other PNG samples after processing.

Compare the same checkpoint across runs with `rupicola diff` or use the optional local panel. Record a concrete observation after inspecting the evidence:

```sh
rupicola capture-review .rupicola/captures/settings-before/manifest.json \
  --note 'Popup becomes visible but keyboard focus stays on the toolbar button.' \
  --evidence checkpoints/opened.png
```

Capture status and inspection status are independent. The command appends a finding and marks inspection recorded. After an authorized fix, rerun the same scenario into a new directory with the updated revision, then inspect and record new observations. If an interrupted review leaves `.review.lock`, remove the empty lock directory only after confirming there is no review in progress.

The [Playwright Browser API](https://playwright.dev/docs/api/class-browser) documents the context settings used here.
