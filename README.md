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

Discovery and skill validation return `1` on diagnostics. A clean scan does not establish UI quality. See [visual verification](reference/visual-verification.md).

Traversal skips hidden directories, nested symlinks, generated output, `target/` and dependency folders. Tests/examples/fixtures/benches are excluded unless `--include-tests` is supplied. An explicit symlink root is resolved before traversal.

Configure `.rupicola/config.toml`:

```toml
ignore_rules = ["gradient-text"]
ignore_paths = ["src/generated"]
```

Paths are relative to the directory containing `.rupicola`. A standalone `// rupicola-ignore transition-all` immediately before the affected line suppresses that rule. Unknown config keys/rules and escaping ignore paths are errors.

Project decisions live in `.rupicola/context.md` when explicitly requested. Visit recording is opt-in. See [project context](reference/project-context.md), [reports](reference/reports.md) and [verification](reference/verification.md).

## Publish captures & work with a backend

```sh
rupicola login --server-url https://rupicola.example.com   # once: asks for the password, stores a token
rupicola publish DIR
```

`rupicola publish DIR` uploads a finished capture bundle (`manifest.json` and its media files, produced by a server-managed or external capture driver) to the backend named by `--server-url`, `RUPICOLA_SERVER_URL` or, failing both, the one `rupicola login` stored. `rupicola whoami` shows who the stored token acts as and `rupicola logout` forgets it; the token stays valid on the backend until it is revoked from a signed-in session. In a Git checkout the project is detected from Git; `rupicola link PROJECT_ID` chooses an explicit ID.

### Work with a backend from the terminal

After `rupicola login`, the CLI reads and writes the backend's resources. Every command takes `--json`, and `--project` defaults to the project this checkout is linked to or Git detects.

```sh
rupicola projects [create ID --label NAME]
rupicola captures [list|show ID|note ID TEXT|run URL]
rupicola problems [list|show ID|create --file -|update ID --status S|comment ID TEXT]
rupicola fixes [list|show ID|create --file F|step ID --file F|review ID --outcome O]
rupicola design [list|add --file F]
```

`create`, `step` and `add` take the JSON of the API's `NewProblem`, `NewFix`, `NewStep` and `NewDesignEntry` (`--file -` reads standard input). Design systems are served by the API and the panel, not yet by the CLI.

The server (`apps/server`) owns accounts, projects, captures, problems, fixes, design records and the media store. The Topcoat panel (`rupicola-panel`) and the CLI (`rupicola`) are clients of its API. See [panel installation](apps/panel/README.md) and [scenario format and inspection](reference/capture-scenarios.md).

## Agents: design context, problems and fixes

Agents reach the backend through the MCP bridge (`rupicola mcp-bridge`, below). Its tools list and record design context (`design_list`, `design_record`), review captures (`capture_review`), file and triage problems (`problem_*`) and record fixes (`fix_*`). Use `brief`, `direction`, `tactic` or `decision` for design records; `supersedes` names the record a new one replaces and `evidence` an artifact of the project's captures.

## Visual QA Pipeline & Web Panel (Captures → Problems → Fixes)

The Topcoat web panel (`rupicola-panel`) organizes visual engineering into a structured pipeline per project, all read from the backend:

- **`Captures` (`/projects/{id}/captures`):** Research hub with screenshots, contact sheets and interaction recordings, a **Screen health** filter (`All captured screens`, `Clean / verified only`, `With problems only`, `Unanalyzed`) and 1-click problem extraction (`Extract problems` / `Flag problem`).
- **`Problems` (`/projects/{id}/problems`):** Structured triage queue linking captured evidence, violated **Global Norms** (`SKILL.md` / `reference/`) and **Project Design Norms**, root-cause diagnosis and proposed solution. Problems marked `Intentional` or `False positive` are suppressed when later captures are analysed.
- **`Fixes` (`/projects/{id}/fixes`):** Before/After verification workbench for screenshots (`.png`) and recordings (`.mp4` / `.webm`) across **`Split Slider`**, **`Side by Side`**, **`Video Player (Slow-Mo 1× / 0.5× / 0.25×)`** and **`Pixel Diff`**, plus applied tactics, modified files, a steps timeline and review (`Approve Fix` / `Request Changes`).
- **`Design` (`/projects/{id}/design`):** The project design register (`brief`, `direction`, `tactic`, `decision`) and versioned design systems.

## Docker & Open-Internet Deployment (Dokploy / Traefik)

The repository ships one multi-stage, `cargo-chef`-cached `Dockerfile` with three targets (`backend`, `panel` and `site`), an open-internet hardened `compose.yml` for Dokploy/Traefik, and a `compose.local.yml` for local testing:

```sh
# Local container test: panel on http://127.0.0.1:4731, backend on http://127.0.0.1:4730
docker compose -f compose.local.yml up -d --build

# Production / VPS behind Dokploy & Traefik (joins external dokploy-network via expose: 4731)
RUPICOLA_PUBLIC_ORIGIN=https://atelier.example.com \
RUPICOLA_DOMAIN=atelier.example.com \
RUPICOLA_SETUP_CODE="$(openssl rand -hex 32)" \
docker compose up -d --build
```

Security posture for open-internet multi-device deployments:
- **Zero raw host port exposure in `compose.yml`:** traffic enters exclusively through Traefik over TLS (`dokploy-network`, container port `4731`).
- **Bootstrap & Auth hardening:** the backend requires `RUPICOLA_SETUP_CODE` (>= 32 chars, SHA-256 constant-time check) for initial owner creation unless it listens on loopback only. Passwords use **Argon2id** with sliding-window rate-limiting (8 failed attempts / 5 min lockout), concurrency semaphore (`429 Too Many Requests`), constant-time dummy verification against user enumeration, `HttpOnly; SameSite=Lax; Secure` session cookies, strict `Host` and `Origin` header pinning, per-session CSRF tokens, and authenticated web password rotation (`Settings → Change password`).
- **Container hardening:** runs as non-root `uid 10001:10001` (`rupicola`) with `read_only: true`, `cap_drop: [ALL]`, `security_opt: ["no-new-privileges:true"]`, `pids_limit: 128`, `mem_limit: 512m`, and a native self-contained `./rupicola-panel --healthcheck` probe.
- **Multi-device capture publishing:** publish from any workstation or CI runner with `rupicola publish <run-dir> --server-url https://atelier.example.com`, using the token `rupicola login` stored (`rupicola auth set-token` stores one issued some other way).

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

The `eval-runner` tool (`tools/eval-runner`, not shipped in releases) runs and analyzes them:

```console
cargo run -p eval-runner -- plan --cond c0=none --cond c1=<git rev> --k 4
cargo run -p eval-runner -- run --cond c0=none --cond c1=<git rev> --model <exact-id> --max-usd 20            # dry run
cargo run -p eval-runner -- run --cond c0=none --cond c1=<git rev> --model <exact-id> --max-usd 20 --execute  # spawns `claude -p`
cargo run -p eval-runner -- analyze evals/results/run --base c0 --cand c1 --csv
```

`run` is a dry run unless `--execute` is given, is resumable (finished runs are skipped), stops at `--max-usd`, and runs the agent with a clean `CLAUDE_CONFIG_DIR` holding only the credential, so user-level skills, hooks, plugins and MCP servers do not leak into any condition (`--no-isolate` disables this). Every run audits the skills the harness reported against the condition; a mismatch counts as a failure. Each run uses a throwaway directory with the skill extracted from the given revision; the agent is allowed Bash there, so prefer a container. `trigger` measures only whether the skill is invoked, over the prompts in `evals/triggers.toml` (30 explicit, 40 implicit, 22 negative; tune on `dev`, judge on `holdout`), and `trigger-analyze` reports precision, recall, F1 and false-positive rate with prompt-cluster bootstrap CIs. The cross-stack update keeps existing prompt IDs and changes eight expected labels. Use the same corpus revision for paired runs. A condition can be `none`, a git revision or `worktree` (current files, to test uncommitted edits). `analyze` reports paired pass rate, a task-cluster bootstrap CI, an exact McNemar test, pass^k and cost; it does not fit mixed-effects models (use `--csv`) and does not yet score review recall or free-text constraints. Skill invocation and false "verified" claims are text heuristics over the trajectory.

## CI and releases

Pushes and pull requests to `main` run formatting, Clippy, skill validation and Rust tests on Linux, Windows and macOS. A separate Linux test entry checks Rust 1.85.0. Dependency and GitHub Actions update PRs are checked by the same workflow.

Pushing a version tag such as `v0.1.0` also runs all checks, verifies the tag matches the package version, builds four target archives, validates each bundled skill, and uploads them with checksums to a GitHub release. Release jobs run only after checks pass. Only the publishing job receives repository write permission; actions are pinned to commit SHAs. No crates.io publishing is configured.

To release, update `Cargo.toml` and `Cargo.lock` together, merge the version change, then tag that commit and push the tag. Do not move a released tag to different code. The release workflow supports retrying asset uploads for the same tag.

The repository is private. No project license has been assigned.

### Hosted MCP for Codex

The backend serves MCP at `POST /mcp`, authenticated with any bearer token it issues: a signed-in session, a machine token or a project device token. `rupicola mcp-bridge --server-url https://YOUR-RUPICOLA-HOST` bridges it to stdio with the token stored by `rupicola login` (or `RUPICOLA_MCP_TOKEN`). A client that speaks HTTP can also be pointed at the endpoint, keeping the bearer in `RUPICOLA_MCP_TOKEN` in the environment that launches it:

```toml
[mcp_servers.rupicola]
url = "https://YOUR-RUPICOLA-HOST/mcp"
bearer_token_env_var = "RUPICOLA_MCP_TOKEN"
```

The endpoint is stateless and checks the token on every request. It exposes `project_list`, `project_create`, `capture_list`, `capture_get`, `capture_run`, `capture_review`, `artifact_read`, `problem_list`, `problem_get`, `problem_create`, `problem_triage`, `problem_comment`, `problem_analyze`, `design_list`, `design_record`, `fix_list`, `fix_get`, `fix_create`, `fix_step` and `fix_review`. Tokens are issued by the backend (`POST /v1/tokens` for a machine, `POST /v1/projects/{id}/devices` for a project device).
