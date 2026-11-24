# Capture bundles & web scenarios

Use either **server-managed execution** (`capture_run` MCP tool, `rupicola captures run`, or the panel form) when the Rupicola server can reach the target HTTP(S) preview URL, or **external local capture + `rupicola publish`** when the preview only runs on client loopback (`127.0.0.1`) or comes from a native toolkit. Start the preview server with your project's normal command and use test accounts and fixture data.

## Server-managed web runs (`capture_run`)

When the backend host has Playwright/Node (`RUPICOLA_PLAYWRIGHT_DIR`) or Chromium and can reach the preview URL, invoke `capture_run` (MCP) or `rupicola captures run URL` (optionally `--steps-file steps.json`). Checkpoints and clips need distinct ASCII IDs (`letters`, `digits`, `-`, `_`). Keep each clip at `2..=60` samples spanning at most `10_000` ms (`interval_ms` `16..=5000`). If `steps` is omitted, the server takes a single `"home"` checkpoint.

Example `steps` JSON array for `capture_run`:

```json
[
  { "kind": "ready", "selector": "#theme-options" },
  { "kind": "checkpoint", "id": "closed" },
  {
    "kind": "clip",
    "id": "open",
    "samples": 16,
    "interval_ms": 40,
    "trigger": { "kind": "click", "selector": "#theme-options" }
  },
  { "kind": "ready", "selector": "#theme-panel" },
  { "kind": "checkpoint", "id": "opened" }
]
```

## External capture bundles & `rupicola publish`

When running Playwright/Chromium or a native capture driver outside the server, produce a v1 bundle (`manifest.json` + media files) and publish it with the CLI:

```sh
# Target project comes from manifest.project.id or --project:
rupicola publish .rupicola/captures/settings-before --project settings-client --server-url https://rupicola.example.com
```

Unlike `captures`/`problems`/`fixes`/`design`, `rupicola publish DIR` reads the target project from `--project` or `[project].id` in `manifest.json` (not Git or `.rupicola/project.toml`). Use a separate capture run for dark theme, a different viewport or reduced motion.

## Inspect a run

When captured via `capture_run`, inspect the stored PNGs and `analysis.json` with `artifact_read` (using the returned `artifact.id` values) or in the panel. For clip recordings, `analysis.json` records the pixel-change heuristic, sample indices, scheduled times and observed screenshot start times relative to the trigger (`0 ms` baseline; retained frames include `video_ms` seek offsets when an MP4 preview exists). Screenshot sampling and CFR MP4 previews provide approximate motion evidence and cannot establish compositor frame pacing.

Compare the same checkpoint across runs in the panel (`Before/After` view). Record a concrete observation after inspecting the evidence with `capture_review` (MCP), `rupicola captures note`, or the panel's review form (`evidence_artifact_id` points at one of the capture's artifact IDs). File deduplicated findings as project problems with `problem_analyze` or `problem_create`.

Capture status and inspection status are independent. After an authorized fix, rerun the same scenario with the updated `revision`, then inspect and record new observations.

The [Playwright Browser API](https://playwright.dev/docs/api/class-browser) documents the browser context settings (`viewport`, `colorScheme`, `reducedMotion`, `locale`).
