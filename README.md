# Ferris Atelier

[![CI](https://github.com/Lucas-BRT/ferris-atelier/actions/workflows/ci.yml/badge.svg)](https://github.com/Lucas-BRT/ferris-atelier/actions/workflows/ci.yml)

A modular AI skill and Rust-native CLI for designing, building and reviewing Rust interfaces across web and desktop. Load only the framework, UX and platform guidance relevant to the task.

The skill covers libcosmic, GPUI, Iced, Topcoat, Leptos, Yew, Tauri, Dioxus, egui/eframe, Slint, GTK4, libadwaita and Relm4. Automated analysis covers a narrower set of syntax patterns; it never certifies runtime behavior or accessibility.

## Install from source

Requirements: Git, a stable Rust toolchain with Cargo, and repository access. The minimum supported Rust version is 1.85.0. Cargo downloads the locked dependencies on the first build. Rust can be installed through [rustup](https://rustup.rs/).

```console
git clone https://github.com/Lucas-BRT/ferris-atelier.git
cd ferris-atelier
cargo install --path . --locked
ferris-atelier --version
```

Ensure Cargo's binary directory is on `PATH`: normally `~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\.cargo\bin` on Windows. If Git credentials are not configured for this private repository, authenticate with GitHub CLI and use `gh repo clone Lucas-BRT/ferris-atelier` instead.

Install just the CLI directly from Git when Git authentication is configured:

```console
cargo install --git https://github.com/Lucas-BRT/ferris-atelier.git --locked
```

The installed CLI runs without Cargo, Node.js, a model endpoint or an API key. Inspected applications still require their own toolchains for compilation and runtime checks. All executable project source and tests are Rust.

## Install a release binary

Version tags trigger builds for Linux x86_64, Windows x86_64 and macOS Apple Silicon/Intel. Archives contain the executable and the complete skill resources. Prebuilt downloads become available after a versioned release succeeds; source installation works independently.

On Linux/macOS, choose the archive matching your machine and an actual published tag:

```console
gh release download <tag> --repo Lucas-BRT/ferris-atelier --pattern 'ferris-atelier-<tag>-<target>.tar.gz'
tar -xzf ferris-atelier-<tag>-<target>.tar.gz
./ferris-atelier/ferris-atelier --version
```

Replace `<target>` with `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` or `x86_64-apple-darwin`. These are placeholders, not literal commands. Keep the extracted directory to use its skill resources, and add it to `PATH` or copy the executable to a directory already on `PATH`.

On Windows, download the `x86_64-pc-windows-msvc` archive from [Releases](https://github.com/Lucas-BRT/ferris-atelier/releases), extract it with `tar -xzf <archive>` or an archive manager, then run `ferris-atelier.exe`. Add its directory to your user `PATH` for subsequent shells.

Each release includes `SHA256SUMS`; compare the downloaded archive's SHA-256 with its listed value. Linux binaries use glibc, Windows binaries use MSVC, and these initial packages are not signed or notarized.

## Install the AI skill

The CLI and the skill are separate: installing a binary does not register the skill. Keep a source checkout or extracted release directory and install its resources under the exact name `ferris-atelier`.

Linux/macOS, from the directory containing `SKILL.md`:

```sh
mkdir -p ~/.agents/skills ~/.claude/skills
ln -s "$PWD" ~/.agents/skills/ferris-atelier
ln -s "$PWD" ~/.claude/skills/ferris-atelier
```

Windows PowerShell, from that same directory:

```powershell
New-Item -ItemType Directory -Force "$HOME\.agents\skills", "$HOME\.claude\skills" | Out-Null
New-Item -ItemType Junction -Path "$HOME\.agents\skills\ferris-atelier" -Target (Get-Location).Path
New-Item -ItemType Junction -Path "$HOME\.claude\skills\ferris-atelier" -Target (Get-Location).Path
```

If a destination already exists, inspect it and update that installation deliberately. The commands do not overwrite existing installations.

Invoke `$ferris-atelier` in Codex or `/ferris-atelier` in Claude Code. [SKILL.md](SKILL.md) contains the reading conditions and direct module links. Keep the CLI on the agent's `PATH` for automated helpers; the guidance remains readable without it.

## CLI commands

```console
ferris-atelier discover /path/to/project --json
ferris-atelier scan /path/to/project --json
ferris-atelier scan page.rs --framework leptos --json
ferris-atelier check /path/to/ferris-atelier --json
ferris-atelier diff before.png after.png --out diff.png
ferris-atelier frames frames-dir
ferris-atelier video frames-dir --out anim.mp4 --fps 60
ferris-atelier video --raw --width 560 --height 780 --fps 60 --pixel-format premultiplied-rgba --save-frames frames-dir --out anim.mp4
ferris-atelier palette reference.png
ferris-atelier sheet shots --out sheet.png
ferris-atelier matrix --cmd '<capture command>' --out shots --states default,empty --themes light,dark --dry-run
ferris-atelier audit tree.json --json
ferris-atelier rules --json
ferris-atelier modules --json
```

`discover` reads Cargo TOML, including aliases, workspace inheritance and target tables. It reports declared normal dependencies, not active features or resolved versions. Dev/build dependencies do not classify the application. It does not invoke Cargo, fetch dependencies, compile or execute project code.

`scan` parses Rust and inspects literal classes in recognized Topcoat/Leptos/Yew/Dioxus macros plus direct blocking calls in UI lifecycle functions. Findings are review candidates. Native semantics, generated code, macro expansion, dynamic classes and Slint/CSS/HTML analysis are outside this scanner's coverage. Read diagnostics, unsupported paths and limitations alongside findings.

| Exit code | Scan result |
|---|---|
| `0` | No candidates in available coverage |
| `1` | Error or diagnostic; takes precedence over candidates |
| `2` | Candidates requiring contextual review |
| `3` | No supported analysis, or unsupported files without candidates |

`diff`, `frames` and `audit` exit `0` when clean and `2` when they find differences or problems; `matrix` exits `1` when a capture failed; `video` exits `0` on a valid MP4 and `1` on input or `ffmpeg` errors (including missing `ffmpeg`). Use `video` and `ferris_atelier::capture::RamCapture` whenever you need to audit UI animations or transitions in the background without Xvfb or touching the user's live desktop: render offscreen frames into RAM at deterministic timestamps (e.g., `iced_tiny_skia` -> `tiny_skia::Pixmap`), measure per-frame CPU raster time, inspect lossless PNGs with `frames`/`sheet`/`diff`, and assemble a CFR MP4 (`libx264`, `yuv420p`, even-dimension padding) with `video` for smooth motion review. Use a real compositor capture (PipeWire `ScreenCast` v5 `persist_mode = 2` + `restore_token`) only when compositor-level behavior (`xdg_popup`, `autosize`, real 60 Hz `wl_callback`) must be verified. These helpers never launch an application themselves (`matrix` runs only the command you give it), and measure, not certify. See [visual verification](reference/visual-verification.md). Discovery and skill validation return `1` on diagnostics. A clean scan does not establish UI quality.

Traversal skips hidden directories, nested symlinks, generated output, `target/` and dependency folders. Tests/examples/fixtures/benches are excluded unless `--include-tests` is supplied. An explicit symlink root is resolved before traversal.

Configure `.ferris-atelier/config.toml`:

```toml
ignore_rules = ["gradient-text"]
ignore_paths = ["src/generated"]
```

Paths are relative to the directory containing `.ferris-atelier`. A standalone `// ferris-ignore transition-all` immediately before the affected line suppresses that rule. Unknown config keys/rules and escaping ignore paths are errors.

Project decisions live in `.ferris-atelier/context.md` when explicitly requested. Visit recording is opt-in. See [project context](reference/project-context.md), [reports](reference/reports.md) and [verification](reference/verification.md).

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

`run` is a dry run unless `--execute` is given, is resumable (finished runs are skipped), stops at `--max-usd`, and runs the agent with a clean `CLAUDE_CONFIG_DIR` holding only the credential, so user-level skills, hooks, plugins and MCP servers do not leak into any condition (`--no-isolate` disables this). Every run audits the skills the harness reported against the condition; a mismatch counts as a failure. Each run uses a throwaway directory with the skill extracted from the given revision; the agent is allowed Bash there, so prefer a container. `trigger` measures only whether the skill is invoked, over the prompts in `evals/triggers.toml` (30 explicit, 30 implicit, 30 negative; tune on `dev`, judge on `holdout`), and `trigger-analyze` reports precision, recall, F1 and false-positive rate with prompt-cluster bootstrap CIs. A condition can be `none`, a git revision or `worktree` (current files, to test uncommitted edits). `analyze` reports paired pass rate, a task-cluster bootstrap CI, an exact McNemar test, pass^k and cost; it does not fit mixed-effects models (use `--csv`) and does not yet score review recall or free-text constraints. Skill invocation and false "verified" claims are text heuristics over the trajectory.

## CI and releases

Pushes and pull requests to `main` run formatting, Clippy, skill validation and Rust tests on Linux, Windows and macOS. A separate Linux test entry checks Rust 1.85.0. Dependency and GitHub Actions update PRs are checked by the same workflow.

Pushing a version tag such as `v0.1.0` also runs all checks, verifies the tag matches the package version, builds four target archives, validates each bundled skill, and uploads them with checksums to a GitHub release. Release jobs run only after checks pass. Only the publishing job receives repository write permission; actions are pinned to commit SHAs. No crates.io publishing is configured.

To release, update `Cargo.toml` and `Cargo.lock` together, merge the version change, then tag that commit and push the tag. Do not move a released tag to different code. The release workflow supports retrying asset uploads for the same tag.

The repository is private. No project license has been assigned.
