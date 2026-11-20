# AGENTS.md — Rupicola

Operational rules for AI agents working in this repository. Never invent files, symbols, endpoints, or test results.

## Toolchain & Workspace
- Rust toolchain: pinned to **`1.99.1`** ([Cargo.toml](Cargo.toml), [.github/workflows/ci.yml](.github/workflows/ci.yml)).
- Workspace members: `.` (`rupicola` CLI + MCP), `panel` (`rupicola-panel` web server + remote MCP), `site` (`rupicola-site`), `xtask`.
- Never add dependencies or alter `Cargo.lock` without explicit need.

## Git, Commits & Releases
- Branches: create `feat/<slug>` or `fix/<slug>` from `develop` → PR to `develop` → PR to `main`.
- `main` is protected (`enforce_admins`): never push directly to `main`.
- Git hooks (`.githooks/`):
  - `commit-msg`: enforces Conventional Commits (`feat|fix|chore|docs|refactor|test|ci|build|perf|style|revert`).
  - `pre-push`: runs `cargo +1.99.1 fmt --check` and workspace Clippy.
- Production deploy (`rupicola.lucasbrt.dev`) triggers **only** on `v*` tags or manual `workflow_dispatch`. Pushes/PRs to `develop` and `main` run CI checks only.

## Local Dev vs Cloud
- Do not install or start local `systemd` services (`rupicola.service` / `ferris-atelier.service`).
- Test panel changes locally via `./panel/dev.sh {reload|stop|reset|status}` (isolated in `.rupicola/dev-data/`, `127.0.0.1:4731`) or `docker compose -f compose.local.yml up --build`.
- Never overwrite `~/.local/share/rupicola/auth-token` from dev scripts.
- Remote MCP: `rupicola mcp-bridge --server-url https://rupicola.lucasbrt.dev/mcp` (reads `~/.local/share/rupicola/auth-token` before `RUPICOLA_MCP_TOKEN`).
- Server MCP endpoints (`panel/src/mcp.rs`): `POST /mcp` (global `machine_tokens`) and `POST /mcp/{project}` (`devices` + `machine_tokens`).

## Mandatory Verification Before Reporting Done
Run and observe passing output after your final edit:
```sh
cargo +1.99.1 fmt --check
cargo +1.99.1 clippy --locked --workspace --all-targets -- -D warnings
cargo +1.99.1 test --locked
cargo +1.99.1 run --locked -- check .
```
Report any skipped or environment-blocked test explicitly (e.g. missing `ffmpeg` in sandbox).

## Skills & Documentation
- Skill catalog lives in [SKILL.md](SKILL.md) and `reference/`. Keep links valid (`rupicola check .` must report `diagnostics: []`).
- State only file paths, symbols, behaviors, and command outputs directly observed in this session.
