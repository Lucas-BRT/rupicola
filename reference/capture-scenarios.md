# Capture bundles & web scenarios

Use either **scenario jobs** (`scenario_put` + `job_create` executed by `rupicola-photographer`) or **direct MCP publication (`capture_publish`)** when capturing locally (e.g. against `127.0.0.1` or a native toolkit). Start the preview server with your project's normal command and use test accounts and fixture data.

## Direct publication via MCP (`capture_publish`)

Agents connected to Rupicola MCP do not need to shell out to the CLI to upload screenshots or recordings:

- Pass `project`, `scenario`, optional `viewport` (`"1280x800"`), `theme` (`"dark"` | `"light"`), `revision`, and `members`.
- Over remote `POST /mcp`, supply `data_base64` per member.
- Over `rupicola mcp-bridge`, you can pass local `file_path` per member (or `bundle_dir` pointing at a `record.json` directory); the bridge streams missing blobs directly over binary HTTP (`PUT /v1/projects/{id}/blobs/{sha256}`) and forwards only the content hashes.
- The response returns the stored `Capture` with every `artifact.id` ready for `problem_create`, `fix_create`, and `capture_review`.

## Declarative web scenarios (`scenario_put` + `job_create`)

Store declarative scenarios in `.rupicola/scenarios/*.toml` (or push them with `scenario_put`) and queue matrix runs with `job_create`. `rupicola-photographer` executes them using `rupicola-protocol`'s in-house Chromium CDP backend (`rupicola-capture`).

Step kinds: `click`, `hover`, `fill`, `press` and `ready` need a `selector` (CSS). `scroll_into_view` needs a `selector` and does not move the pointer. `scroll` (`dx`, `dy` pixels), `scroll_to` (`x`, `y` offset) and `scroll_to_end` take an optional `selector`: the page when absent, otherwise that element (`scroll` uses its closest ancestor that scrolls). `wait` pauses `ms` milliseconds, at most `10_000` each and `30_000` in total. `checkpoint` takes `full_page: true` to shoot the whole scrollable page instead of the viewport. `clip` records `2..=60` samples spanning at most `10_000` ms (`interval_ms` `16..=5000`) around a `trigger` action.

## CLI fallback (`rupicola publish`)

When publishing a finished canonical `record.json` bundle outside MCP:

```sh
rupicola publish .rupicola/captures/settings-before --project settings-client --server-url https://rupicola.example.com
```

`rupicola publish DIR` reads `record.json` and uploads only missing SHA-256 member blobs.

## Inspect a run

Inspect stored PNGs with `artifact_read` (using the returned `artifact.id` values) or in the panel. Compare the same checkpoint across runs in the panel (`Before/After` view). Record a concrete observation after inspecting the evidence with `capture_review` (`evidence_artifact_id` points at one of the capture's artifact IDs), and file deduplicated findings as project problems with `problem_analyze` or `problem_create`.

Capture status and inspection status are independent. After an authorized fix, rerun the same scenario with the updated `revision`, then inspect and record new observations.

## Progressive Video & Visual Inspection (Token-Efficient MCP Ladder)

When inspecting recorded UI clips (such as 60 FPS transitions) or comparing before/after screenshots through MCP, follow this ladder to avoid flooding context with raw frames:

1. **`media_probe`** — read dimensions, duration, FPS, and total frame count (0 images in context).
2. **`media_motion_diff`** — detect numeric pixel/luminance spikes (flashes, flickers, layout jumps) as pure JSON before opening any PNG.
3. **`media_contact_sheet`** — render one `cols x rows` contact-sheet PNG for single-image visual triage.
4. **`media_sample_frames`** — extract frames progressively (`strategy: "coarse"` → `"bisect"` with `exclude_frames` → `"window"` at 60 FPS strictly around the suspect timestamps), then inspect only the targeted frames with `artifact_read`.
5. **`visual_diff_images` + `visual_crop_region`** — compute changed bounding boxes and inspect only cropped regions instead of full-page screenshots.
