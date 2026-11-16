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

Discovery and skill validation return `1` on diagnostics. A clean scan does not establish UI quality.

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

`check` validates required modules, direct links, local documentation links and source records. Tests exercise offline discovery, parsing, suppressions, traversal, diagnostics and CLI exit codes. They do not run the UI frameworks or evaluate AI-host behavior. The guides document patterns rather than shipping compiled framework application samples. Official reference URLs and verification limits are recorded in [source records](reference/sources.toml).

## CI and releases

Pushes and pull requests to `main` run formatting, Clippy, skill validation and Rust tests on Linux, Windows and macOS. A separate Linux test entry checks Rust 1.85.0. Dependency and GitHub Actions update PRs are checked by the same workflow.

Pushing a version tag such as `v0.1.0` also runs all checks, verifies the tag matches the package version, builds four target archives, validates each bundled skill, and uploads them with checksums to a GitHub release. Release jobs run only after checks pass. Only the publishing job receives repository write permission; actions are pinned to commit SHAs. No crates.io publishing is configured.

To release, update `Cargo.toml` and `Cargo.lock` together, merge the version change, then tag that commit and push the tag. Do not move a released tag to different code. The release workflow supports retrying asset uploads for the same tag.

The repository is private. No project license has been assigned.
