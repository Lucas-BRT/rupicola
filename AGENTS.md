# AGENTS.md — Rupicola

Operational rules for AI agents working in this repository. Never invent files, symbols, endpoints, or test results.

- Add dependencies or change `Cargo.lock` only when needed.

## Git, Commits & Releases
- Branches: create `feat/<slug>` or `fix/<slug>` from `develop` → PR to `develop` → PR to `main`.
- `main` is protected (`enforce_admins`): never push directly to `main`.
- Git hooks (`.githooks/`):
  - `commit-msg`: enforces Conventional Commits (`feat|fix|chore|docs|refactor|test|ci|build|perf|style|revert`).
  - `pre-push`: runs formatting and workspace Clippy checks.
- Production deploy triggers **only** on `v*` tags or manual `workflow_dispatch`. Pushes/PRs to `develop` and `main` run CI checks only.

## Local Dev vs Cloud
- Do not install or start local `systemd` services (`rupicola.service` / `ferris-atelier.service`).
- Test panel changes locally via `./apps/panel/dev.sh {reload|stop|reset|status}` (isolated in `.rupicola/dev-data/`, `127.0.0.1:4731`) or `docker compose -f compose.local.yml up --build`.
- Never overwrite `~/.local/share/rupicola/auth-token` from dev scripts.
- Remote MCP: `rupicola mcp-bridge` (backend address from `RUPICOLA_SERVER_URL` or `--server-url`) (reads `~/.local/share/rupicola/auth-token` before `RUPICOLA_MCP_TOKEN`).
- `rupicola capture` and `probe` publish the finished capture to the backend when `RUPICOLA_SERVER_URL` or `--server-url` is set; without `--out` they capture into a scratch directory that is removed afterwards.
- Server MCP endpoints (`apps/panel/src/mcp.rs`): `POST /mcp` (global `machine_tokens`) and `POST /mcp/{project}` (`devices` + `machine_tokens`).

## Mandatory Verification Before Reporting Done
Run and observe passing output after your final edit:
```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- check .
```
Report any skipped or environment-blocked test explicitly (e.g. missing `ffmpeg` in sandbox).

## Skills & Documentation
- Skill catalog lives in [SKILL.md](SKILL.md) and `reference/`. Keep links valid (`rupicola check .` must report `diagnostics: []`).
- State only file paths, symbols, behaviors, and command outputs directly observed in this session.
