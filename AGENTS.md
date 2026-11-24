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
- Test panel changes locally via `./apps/panel/dev.sh {reload|stop|status}` (`127.0.0.1:4731`) or `docker compose -f compose.local.yml up --build`.
- Never overwrite `~/.local/share/rupicola/auth-token` from dev scripts.
- Remote MCP: `rupicola mcp-bridge` (backend address from `--server-url`, `RUPICOLA_SERVER_URL` or the `server-url` file `rupicola login` writes next to the token) (reads `~/.local/share/rupicola/auth-token` before `RUPICOLA_MCP_TOKEN`). Run the CLI against a dev backend with `XDG_DATA_HOME` pointing at a scratch directory so `login` does not replace your token.
- `rupicola publish <dir>` uploads a finished capture bundle to the backend when `--server-url`, `RUPICOLA_SERVER_URL` or a stored `server-url` names one.
- Server MCP endpoint (`apps/server/src/http/mcp.rs`): `POST /mcp`, authenticated with any bearer token the backend issues. The panel serves no MCP.

## Mandatory Verification Before Reporting Done

Run and observe passing output after your final edit:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo nextest run --locked --workspace
cargo run --locked -- check .
```

Report any skipped or environment-blocked test explicitly (e.g. missing `ffmpeg` in sandbox).

## Skills & Documentation

- Skill catalog lives in [SKILL.md](SKILL.md) and `reference/`. Keep links valid (`rupicola check .` must report `diagnostics: []`).
- State only file paths, symbols, behaviors, and command outputs directly observed in this session.

## Capture Boundary & Protocol Issues

- **Strict capture boundary**: This repository implements ZERO capture drivers/engines and bundles ZERO browsers or desktop capture packages. ALL captures belong exclusively to `rupicola-protocol` (`rupicola-capture`); `rupicola` only consumes it as an external crate and surfaces its capabilities to end users.
- **Zero-install default + OS detection**: Never install Chromium, Playwright, or desktop capture packages by default (including in Docker images). Detect compatible browsers and desktop capture packages already on the host OS, visually report available capture options, and guide users on OS installation when absent.
- **Mandatory GitHub issue procedure**: Whenever a real capture bug, missing capability, or limitation is found, open a GitHub issue on `Lucas-BRT/rupicola-protocol` with: (1) detailed motivation/root cause, (2) a concise reproducible code example acting as context/test case, and (3) a suggested fix/API design when available.
- **Prototype freedom**: While in prototype development, breaking contracts, resetting/remodeling the database schema from scratch, and deleting legacy compatibility code are explicitly allowed in favor of a clean, extensible final architecture.
