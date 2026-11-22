# Rupicola

[![CI](https://github.com/Lucas-BRT/rupicola/actions/workflows/ci.yml/badge.svg)](https://github.com/Lucas-BRT/rupicola/actions/workflows/ci.yml)

A modular AI skill and Rust-native CLI for designing, building and reviewing graphical interfaces across web, native desktop and webviews. Load only the framework, UX and platform guidance relevant to the task.

The skill covers libcosmic, GPUI, Iced, Topcoat, Leptos, Yew, Tauri, Dioxus, egui/eframe, Slint, GTK4, libadwaita and Relm4. Automated analysis covers a narrower set of syntax patterns; it never certifies runtime behavior or accessibility.

## Install from source

Requirements: Git, Rust 1.99 or newer with Cargo, and repository access. Cargo downloads the locked dependencies on the first build. Rust can be installed through [rustup](https://rustup.rs/).

```console
git clone https://github.com/Lucas-BRT/rupicola.git
cd rupicola
cargo install --path apps/cli --locked
rupicola --version
```

Ensure Cargo's binary directory is on `PATH`: normally `~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\.cargo\bin` on Windows. If Git credentials are not configured for this private repository, authenticate with GitHub CLI and use `gh repo clone Lucas-BRT/rupicola` instead.

Install just the CLI directly from Git when Git authentication is configured:

```console
cargo install --git https://github.com/Lucas-BRT/rupicola.git --locked rupicola
```

The installed CLI needs neither Cargo nor a model API key. Core analysis commands run without Node.js. Headless web capture uses Node.js and Playwright. The local review panel is an optional executable. Inspected applications still require their own toolchains for compilation and runtime checks.

To enable Rupicola in a project, add an opt-in instruction to its `AGENTS.md`:

> Use Rupicola for visual or interaction changes. Capture and inspect the affected UI states, recapture after authorized fixes, and report any blocked checks. API-only work is outside this workflow.

## Install a release binary

Version tags trigger builds for Linux x86_64, Windows x86_64 and macOS Apple Silicon/Intel. Archives contain the executable and the complete skill resources. Prebuilt downloads become available after a versioned release succeeds; source installation works independently.

On Linux/macOS, choose the archive matching your machine and an actual published tag:

```console
gh release download <tag> --repo Lucas-BRT/rupicola --pattern 'rupicola-<tag>-<target>.tar.gz'
tar -xzf rupicola-<tag>-<target>.tar.gz
./rupicola/rupicola --version
```

Replace `<target>` with `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` or `x86_64-apple-darwin`. These are placeholders, not literal commands. Keep the extracted directory to use its skill resources, and add it to `PATH` or copy the executable to a directory already on `PATH`.

On Windows, download the `x86_64-pc-windows-msvc` archive from [Releases](https://github.com/Lucas-BRT/rupicola/releases), extract it with `tar -xzf <archive>` or an archive manager, then run `rupicola.exe`. Add its directory to your user `PATH` for subsequent shells.

Each release includes `SHA256SUMS`; compare the downloaded archive's SHA-256 with its listed value. Linux binaries use glibc, Windows binaries use MSVC, and these initial packages are not signed or notarized.

## Install the AI skill

The CLI and the skill are separate: installing a binary does not register the skill. Keep a source checkout or extracted release directory and install its resources under the exact name `rupicola`.

Linux/macOS, from the directory containing `SKILL.md`:

```sh
mkdir -p ~/.agents/skills ~/.claude/skills
ln -s "$PWD" ~/.agents/skills/rupicola
ln -s "$PWD" ~/.claude/skills/rupicola
```

Windows PowerShell, from that same directory:

```powershell
New-Item -ItemType Directory -Force "$HOME\.agents\skills", "$HOME\.claude\skills" | Out-Null
New-Item -ItemType Junction -Path "$HOME\.agents\skills\rupicola" -Target (Get-Location).Path
New-Item -ItemType Junction -Path "$HOME\.claude\skills\rupicola" -Target (Get-Location).Path
```

If a destination already exists, inspect it and update that installation deliberately. The commands do not overwrite existing installations.

Invoke `$rupicola` in Codex or `/rupicola` in Claude Code. [SKILL.md](SKILL.md) contains the reading conditions and direct module links. Keep the CLI on the agent's `PATH` for automated helpers; the guidance remains readable without it.

## CLI commands

```console
rupicola discover /path/to/project --json
rupicola scan /path/to/project --json
rupicola scan page.rs --framework leptos --json
rupicola check /path/to/rupicola --json
rupicola probe --url http://127.0.0.1:3000 --mode exhaustive --pages all --viewports 320x844,768x1024,1440x900 --themes dark,light --json
rupicola problem list --project my-app --json
rupicola problem claim <problem-id> --agent codex-ui --activity "Inspecting evidence and violated norms"
rupicola problem progress <problem-id> --agent codex-ui --stage verifying --step 3 --total-steps 4 --pct 80 --activity "Running Playwright matrix verification"
rupicola fix record --project my-app --problem <problem-id> --title "Fix hairline seams" --summary "Unified 1px grid seams" --before-url /media/0/before/shot.png --after-url /media/0/after/shot.png
rupicola propose list --project my-app --json
rupicola storage status --project my-app
rupicola storage clean --mode strip-frames --project my-app
rupicola storage offload --fix <fix-id> --project my-app
rupicola diff before.png after.png --out diff.png
rupicola frames frames-dir
rupicola video frames-dir --out anim.mp4 --fps 60
rupicola video --raw --width 560 --height 780 --fps 60 --pixel-format premultiplied-rgba --save-frames frames-dir --out anim.mp4
rupicola palette reference.png
rupicola sheet shots --out sheet.png
rupicola matrix --cmd '<capture command>' --out shots --states default,empty --themes light,dark --dry-run
rupicola audit tree.json --json
rupicola rules --json
rupicola modules --json
```

`discover` reads Cargo TOML, including aliases, workspace inheritance and target tables. It reports declared normal dependencies, not active features or resolved versions. Dev/build dependencies do not classify the application. It does not invoke Cargo, fetch dependencies, compile or execute project code.

`scan` parses Rust and inspects literal classes in recognized Topcoat/Leptos/Yew/Dioxus macros plus direct blocking calls in UI lifecycle functions. Findings are review candidates. Native semantics, generated code, macro expansion, dynamic classes and Slint/CSS/HTML analysis are outside this scanner's coverage. Read diagnostics, unsupported paths and limitations alongside findings.

| Exit code | Scan result |
|---|---|
| `0` | No candidates in available coverage |
| `1` | Error or diagnostic; takes precedence over candidates |
| `2` | Candidates requiring contextual review |
| `3` | No supported analysis, or unsupported files without candidates |

`diff`, `frames` and `audit` exit `0` when clean and `2` when they find differences or problems; `matrix` exits `1` when a capture failed; `video` exits `0` on a valid MP4 and `1` on input or `ffmpeg` errors (including missing `ffmpeg`). Use `video` and `rupicola::capture::RamCapture` whenever you need to audit UI animations or transitions in the background without Xvfb or touching the user's live desktop: render offscreen frames into RAM at deterministic timestamps (e.g., `iced_tiny_skia` -> `tiny_skia::Pixmap`), measure per-frame CPU raster time, inspect lossless PNGs with `frames`/`sheet`/`diff`, and assemble a CFR MP4 (`libx264`, `yuv420p`, even-dimension padding) with `video` for smooth motion review. Use a real compositor capture (PipeWire `ScreenCast` v5 `persist_mode = 2` + `restore_token`) only when compositor-level behavior (`xdg_popup`, `autosize`, real 60 Hz `wl_callback`) must be verified. These helpers never launch an application themselves (`matrix` runs only the command you give it), and measure, not certify. See [visual verification](reference/visual-verification.md). Discovery and skill validation return `1` on diagnostics. A clean scan does not establish UI quality.

Traversal skips hidden directories, nested symlinks, generated output, `target/` and dependency folders. Tests/examples/fixtures/benches are excluded unless `--include-tests` is supplied. An explicit symlink root is resolved before traversal.

Configure `.rupicola/config.toml`:

```toml
ignore_rules = ["gradient-text"]
ignore_paths = ["src/generated"]
```

Paths are relative to the directory containing `.rupicola`. A standalone `// rupicola-ignore transition-all` immediately before the affected line suppresses that rule. Unknown config keys/rules and escaping ignore paths are errors.

Project decisions live in `.rupicola/context.md` when explicitly requested. Visit recording is opt-in. See [project context](reference/project-context.md), [reports](reference/reports.md) and [verification](reference/verification.md).

## Capture and review a web scenario

```sh
rupicola capture settings.toml --out .rupicola/captures/before --video
rupicola capture-review .rupicola/captures/before/manifest.json --note "The popup opens and its heading is visible."
cargo install --path apps/panel --locked
rupicola serve --captures .rupicola/captures
```

The headless capture stores checkpoints, clip analysis, selected PNG frames and optional MP4 previews. The Topcoat panel indexes multiple capture roots and supports shared review behind an HTTPS reverse proxy. See [panel installation](apps/panel/README.md), [scenario format and inspection](reference/capture-scenarios.md) and the [nested COSMIC applet spike](reference/libcosmic-capture-spike.md). `capture` handles HTTP(S) previews. On Linux, `capture-cosmic` uses an isolated panel host for two native applet checkpoints.

## Run the local panel in the background

On Linux, install the CLI and panel once, then register the panel with the user
systemd manager:

```sh
cargo install --path apps/cli --locked
cargo install --path apps/panel --locked
rupicola setup
```

Run captures from a Git checkout without `--out`. Rupicola detects the local Git
repository, groups its worktrees under the same ID and writes evidence to the
shared directory. Use `rupicola link PROJECT_ID` to choose an explicit ID. Captures and inspection notes
remain available on disk while the service is stopped and appear when the
service resumes. Use `rupicola doctor` to check the link and service.
`setup --print-unit` previews the unit.

Open http://127.0.0.1:4731. The background service is local. Shared HTTPS
publication still uses the [panel's explicit publish command](apps/panel/README.md).

## Record project design context

The skill calls `rupicola design open` from the checkout before design work.
The project appears in **Projects** on the local panel even with no captures.

```sh
rupicola design open
rupicola design add direction --title "Graphite and copper" \
  --note "Keep capture media prominent and use copper for the review action." --surface "Review"
rupicola design add tactic --status proposed --title "Aligned comparison" \
  --note "Put paired screenshots on equal tracks when the review pane has room."
```

Use `brief`, `direction`, `tactic` or `decision`. Add `--supersedes ENTRY_ID`
to revise a record while preserving history, `--reference URL` to attach a
design reference, or `--evidence /path/to/manifest.json` to attach a capture
stored in the same project's shared directory.

The panel reads the design register beside its database. For the per-user
service, this is the same local data directory that the CLI uses. Shared
HTTPS capture publication does not sync the design register.

## Visual QA Pipeline & Web Panel (Captures → Problems → Fixes → Storage)

The Topcoat web panel (`rupicola-panel`) organizes visual engineering into a structured pipeline per project:

- **`Captures` (`/projects/{id}/captures`):** Unified research hub combining matrix runs, screenshots, contact sheets, and interaction recordings with a **Screen health** filter (`All captured screens`, `Clean / verified only`, `With problems only`, `Unanalyzed`), inline `Run inspection matrix` launcher (`page-check`, `all-pages`, `responsive-matrix`, `interaction-map`, `exhaustive`), and 1-click problem extraction (`Extract problems` / `Flag problem`).
- **`Problems` (`/projects/{id}/problems`):** Structured triage queue linking captured evidence assets, violated **Global Norms** (`SKILL.md` / `reference/`) and **Project Design Norms**, root-cause diagnosis, proposed solution, and a live **Agent & Task Execution Tracker** (`AGENT`, `STAGE`, `STEP X / Y · Z%`, `<progress>` bar, activity log, and `2.5s` live polling via `/projects/{id}/problems/status.json`). Problems marked `Intentional` or `False positive` are automatically suppressed in subsequent probes.
- **`Fixes` (`/projects/{id}/fixes`):** Interactive Before/After verification workbench supporting both static screenshots (`.png`) and motion recordings (`.mp4` / `.webm`) across **`Split Slider`**, **`Side by Side`**, **`Video Player (Slow-Mo 1× / 0.5× / 0.25×)`**, and **`Pixel Diff`**, plus applied tactics, modified files, intermediate steps timeline, and user approval (`Approve Fix` / `Request Changes`).
- **`Design` (`/projects/{id}/design`):** The project design register, versioned systems, interactive screen templates, and proposals. The old Proposes list redirects to Design; existing HTML previews and attachments keep their URLs.

### Review a design system

Open **Design → System** and create a draft. Fill in identity and usage, semantic colors, typography, spacing, component guidance, and template content. **Screens** renders a live component library plus Home, Work queue, Settings, and Evidence review examples with the same tokens. The queue filters real example rows, settings update the demo, and evidence uses bundled images and WebM clips. Example actions do not write project data.

Select **Validate examples**. The panel checks token syntax and contrast, then drives Chromium at 320 and 1440 CSS pixels through the component states, landing navigation, queue filters and empty state, settings form, and image and clip comparison. The report links captures of each example. When a system is already adopted, it captures that baseline with the same content and viewport. If the browser is missing, validation is blocked. Saving a change invalidates the previous report, and an unvalidated draft cannot be submitted. Submitting freezes the preview. A change to the adopted baseline before submission requires validation again. Review, variations, and explicit adoption live under **Design → Proposals**.

The panel Docker image includes the browser runner. For a native panel, install Node.js and Playwright on the panel host with its Chromium browser. If Playwright is not available via Node's default module lookup, set `RUPICOLA_PLAYWRIGHT_MODULE` to the absolute path of the installed module. `RUPICOLA_CHROMIUM` optionally selects a system Chromium executable.
- **`Storage` (`/projects/{id}/storage`):** Disk usage telemetry grouped **By Fix**, **By Date**, **By Capture Run**, and **By Asset Category**, with selective cleanup (`Strip raw clip frames`, `Prune unpinned runs`) and external bucket offload (`S3-Compatible / Cloudflare R2 / MinIO / B2`, `rclone`, or mounted volume) with optional automatic offload when a Fix is approved.

## Docker & Open-Internet Deployment (Dokploy / Traefik)

The repository ships one multi-stage, `cargo-chef`-cached `Dockerfile` with two targets (`panel` and `site`), an open-internet hardened `compose.yml` for Dokploy/Traefik, and a `compose.local.yml` for local testing:

```sh
# Local container test on http://127.0.0.1:4731
docker compose -f compose.local.yml up -d --build

# Production / VPS behind Dokploy & Traefik (joins external dokploy-network via expose: 4731)
RUPICOLA_PUBLIC_ORIGIN=https://atelier.example.com \
RUPICOLA_SETUP_CODE="$(openssl rand -hex 32)" \
docker compose up -d --build
```

Security posture for open-internet multi-device deployments:
- **Zero raw host port exposure in `compose.yml`:** traffic enters exclusively through Traefik over TLS (`dokploy-network`, container port `4731`).
- **Bootstrap & Auth hardening:** `RUPICOLA_PUBLIC_ORIGIN` (`https://...`) requires `RUPICOLA_SETUP_CODE` (>= 32 chars, SHA-256 constant-time check) for initial owner creation. Passwords use **Argon2id** with sliding-window rate-limiting (8 failed attempts / 5 min lockout), concurrency semaphore (`429 Too Many Requests`), constant-time dummy verification against user enumeration, `HttpOnly; SameSite=Lax; Secure` session cookies, strict `Host` and `Origin` header pinning, per-session CSRF tokens, and authenticated web password rotation (`Settings → Change password`).
- **Container hardening:** runs as non-root `uid 10001:10001` (`rupicola`) with `read_only: true`, `cap_drop: [ALL]`, `security_opt: ["no-new-privileges:true"]`, `pids_limit: 128`, `mem_limit: 512m`, and a native self-contained `./rupicola-panel --healthcheck` probe.
- **Multi-device capture publishing:** generate per-machine credentials in `Project settings → Machines` and publish from any workstation or CI runner with `RUPICOLA_PUBLISH_TOKEN=<token> rupicola-panel publish <run-dir> --server https://atelier.example.com`.

## Development

```console
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- check .
cargo build --release --locked
```

`check` validates required modules, direct links, local documentation links, source records and evaluation task definitions under `evals/tasks/`. Tests exercise offline discovery, parsing, suppressions, traversal, diagnostics and CLI exit codes. They do not run the UI frameworks or evaluate AI-host behavior. The guides document patterns rather than shipping compiled framework application samples. Official reference URLs and verification limits are recorded in [source records](reference/sources.toml).

## Examples

`examples/` holds one crate per toolkit (egui, Slint, Iced, GTK4 with libadwaita, GPUI, libcosmic) that reproduces web-style effects natively: a popover (anchored with flip and shift, or by the toolkit widget), a shared-element transition (not in libcosmic), and a scroll-linked reveal. Each crate is its own workspace, excluded from the main one, and records its dependency versions in its lockfile; GPUI carries a one-line patched copy of `xattr` (see its comment in `Cargo.toml`). All six were run and inspected as images: egui (`egui_kittest`) and Iced (`iced_test`) render offscreen on software Vulkan, and Slint, GPUI, GTK4 and libcosmic were run on a virtual X display and captured; each is also compile-checked (clippy clean). Bugs found this way are listed in each framework module: `cargo check` on a machine that has the toolkits' system libraries, or, where those are missing, with stub `.pc` files on `PKG_CONFIG_PATH` (which suffices for `cargo check`, not for linking). The inspection used software rendering on a virtual display, so it does not cover GPU rendering, a real compositor, transparency or Wayland behavior.

## Evaluation tasks

`evals/tasks/<id>/task.toml` defines a measurable task: prompt, framework, kind (`build`, `review`, `improve`, `plan`, `trigger`), `dev`/`holdout` split, constraints, deterministic verifiers and, for reviews, seeded defects with fixtures. `check` validates the schema offline; it does not run a model or the verifiers. Copy [evals/PREREG.md](evals/PREREG.md) and commit it before each measured run. The 10 tasks present are pilot definitions only; no baseline result or statistical conclusion exists yet.

The `xtask` crate (not shipped in releases) runs and analyzes them:

```console
cargo run -p xtask -- plan --cond c0=none --cond c1=<git rev> --k 4
cargo run -p xtask -- run --cond c0=none --cond c1=<git rev> --model <exact-id> --max-usd 20            # dry run
cargo run -p xtask -- run --cond c0=none --cond c1=<git rev> --model <exact-id> --max-usd 20 --execute  # spawns `claude -p`
cargo run -p xtask -- analyze evals/results/run --base c0 --cand c1 --csv
```

`run` is a dry run unless `--execute` is given, is resumable (finished runs are skipped), stops at `--max-usd`, and runs the agent with a clean `CLAUDE_CONFIG_DIR` holding only the credential, so user-level skills, hooks, plugins and MCP servers do not leak into any condition (`--no-isolate` disables this). Every run audits the skills the harness reported against the condition; a mismatch counts as a failure. Each run uses a throwaway directory with the skill extracted from the given revision; the agent is allowed Bash there, so prefer a container. `trigger` measures only whether the skill is invoked, over the prompts in `evals/triggers.toml` (30 explicit, 40 implicit, 22 negative; tune on `dev`, judge on `holdout`), and `trigger-analyze` reports precision, recall, F1 and false-positive rate with prompt-cluster bootstrap CIs. The cross-stack update keeps existing prompt IDs and changes eight expected labels. Use the same corpus revision for paired runs. A condition can be `none`, a git revision or `worktree` (current files, to test uncommitted edits). `analyze` reports paired pass rate, a task-cluster bootstrap CI, an exact McNemar test, pass^k and cost; it does not fit mixed-effects models (use `--csv`) and does not yet score review recall or free-text constraints. Skill invocation and false "verified" claims are text heuristics over the trajectory.

## CI and releases

Pushes and pull requests to `main` run formatting, Clippy, skill validation and Rust tests on Linux, Windows and macOS. A separate Linux test entry checks Rust 1.85.0. Dependency and GitHub Actions update PRs are checked by the same workflow.

Pushing a version tag such as `v0.1.0` also runs all checks, verifies the tag matches the package version, builds four target archives, validates each bundled skill, and uploads them with checksums to a GitHub release. Release jobs run only after checks pass. Only the publishing job receives repository write permission; actions are pinned to commit SHAs. No crates.io publishing is configured.

To release, update `Cargo.toml` and `Cargo.lock` together, merge the version change, then tag that commit and push the tag. Do not move a released tag to different code. The release workflow supports retrying asset uploads for the same tag.

The repository is private. No project license has been assigned.

### Hosted MCP for Codex

Use a project device token issued in the hosted panel. Keep the bearer in
`RUPICOLA_MCP_TOKEN` in the environment that launches Codex. Replace the local
`command` and `args` configuration with:

```toml
[mcp_servers.rupicola]
url = "https://YOUR-RUPICOLA-HOST/mcp"
bearer_token_env_var = "RUPICOLA_MCP_TOKEN"
```

Restart Codex after the panel version with this endpoint is deployed.
The stateless endpoint lists published captures, problems, and fixes for the
token's project. It checks the token on every request. Revoke the device in
the panel to disable access. Device tokens also authorize the existing capture
publish API for their project. The hosted MCP defaults to read access. Set `RUPICOLA_MCP_WRITE=1` on the panel host to expose write tools to owner machine tokens on `/mcp`. Project devices and non-owner tokens remain read-only. `problem_create` and `fix_create` require a `project`, a stable retry `key`, and a `record`. A repeated key returns the existing record. Use published, same-project `/media/…` URLs for evidence. Fixes enter pending review and leave the status of linked Problems unchanged.

Write access also exposes `problem_analyze`, `problem_triage`, `problem_progress`, `fix_review`, `capture_review`, `propose_create`, `propose_enrich`, `propose_review`, `project_create`, `design_record`, `design_system_action`, and `design_system_edit`. Use `design_system_get` to obtain the current generation. Owner tokens can edit, validate, submit, comment, approve, request changes and adopt. Review and adoption require the version and current generation. Use `capture_publish` for capture bundles up to 64 MiB including the JSON-RPC envelope, subject to the publishing API’s media limits.


Use `design_system_preview` for rendered HTML and `design_system_proof` with
`proof: "version/name"` for a validation screenshot. Use `propose_asset_read`
with `asset: "proposal_id/filename"` to read a declared attachment or preview,
and `propose_dispatch_list` to inspect up to 25 queued requests without consuming
them. Proof and proposal asset responses are capped at 4 MiB of base64.

`storage_status` includes storage totals and usage by run and Fix.
`storage_run_action` requires owner write access. It offloads a single capture
to the configured bucket or deletes a single unpinned capture under the panel's
storage directory.

`project_rename` changes the display label while retaining the project ID.
`project_access` lists members, devices and recent audit events, issues
project invites or device credentials, and revokes access. Both tools require
owner write access. Treat newly issued credentials as secrets.

Use `artifact_read_chunk` with `chunk: "0:/media/…"` to read larger declared
capture artifacts. Repeat with the returned `next_offset` until it is null.

`storage_config` reads the panel-wide bucket configuration and replaces it
when called with a full `config` object. It requires owner write access.
Set mounted filesystem paths in the panel.

`project_merge_preview` accepts `source` and `target` project IDs and requires
owner write access. It reports project file inventory, capture counts, and
database records and overlaps. It leaves both projects untouched.

`project_merge_apply` merges a registered source into a registered target,
including capture manifests, JSON records, proposal assets and dispatches.
Media URLs and record IDs are preserved. Active proposal sessions and
conflicting files block execution. Set `revoke_source_access: true` to revoke
source invites, device credentials and memberships during the merge. Target
access stays intact. By default, incompatible source access blocks execution.
Retry an interrupted merge with the same IDs to resume its capture journal. Use
`project_merge_preview` first to review both projects.
