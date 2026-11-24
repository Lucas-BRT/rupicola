# Handoff: scenarios, runners and isolated fix comparisons

For the next agent. Read this whole file, then `capture-runner.md` (the design), then start at
"Next steps". Everything here was observed in the session that wrote it; where something is a
guess, it says so.

## 1. What this is for

Rupicola records problems and fixes for a UI, and a fix is proved by before/after captures. Two
things went wrong:

1. **Fixes contaminated each other.** All the fixes of one round were captured on a single
   branch that had every fix applied, so the "after" of fix X also showed fixes Y and Z. The
   user approves fixes one at a time in the panel, and approving X changes how Y looks, so the
   comparison on Y is no longer what Y does.
2. **Captures were written by an agent each time.** That costs tokens and varies between runs.

The goal, agreed with the user:

- A capture is a **TOML file the repository owns** (`.rupicola/scenarios/*.toml`). The agent
  writes the file; running it again is plain code, no tokens.
- Running it is a **job** anyone can request through MCP, the CLI or the panel.
- A **runner** process on a machine that has the code builds the trees, captures with
  `rupicola-capture`, and uploads the result. The server never builds or runs project code, so
  the same design works installed on a laptop, in Docker, or on a cloud host that cannot reach a
  developer's `127.0.0.1`.
- Each fix is compared **isolated** (base against base + this fix alone) and **integrated**
  (base + every approved fix against the same + this fix). Approving a fix makes the other
  fixes' integrated comparisons **stale**, and by default they are **recaptured
  automatically**. The panel shows when matrix captures are still running.

## 2. Decisions the user made (do not re-litigate)

- The repository's code and files are the source of truth, because visual changes also happen
  without Rupicola. Scenario files live in the repo; the server only keeps pushed copies.
- Format is **TOML**, validated by the server, with JSON served to MCP clients.
- The **runner** model is the primary architecture (not server-built previews), because a cloud
  server cannot capture a local dev tree. The runner captures and uploads.
- The server must be installable on its own, not only in Docker. On a laptop it may start a
  runner in-process (not built yet).
- Checks (see slice 6) are in scope now, not later.
- Automatic recapture after an approval is **on by default**, with an option to disable it, and
  the panel must show that matrix captures are still in progress.
- For themes inside the matrix the user said "do what you recommend": a theme with no setup
  steps sets the browser's color scheme; a theme with steps runs them (the landing page pins
  `data-theme="dark"`, so its light theme is a click on `[data-theme-toggle]`).
- **Do not push or open PRs without asking.** For the landing-page UI fixes the user said the PR
  waits until the UI/UX is completely clean (section 7).

## 3. State of the work

### Repositories and branches

`rupicola` (this repo, `/home/lucasbrt/Projects/rupicola`), all local, none pushed:

| Branch | Based on | Contents |
| --- | --- | --- |
| `develop` (`e236472`) | | Has PR #25: `capture_run` takes `scroll`, `scroll_into_view`, `scroll_to`, `scroll_to_end`, `wait` steps and `full_page` checkpoints; `Cargo.lock` pins the protocol at `e9c748e`. |
| `feat/fix-comparison-views` (`e061b08`) | `develop` | Comparison `view`, revisions, `applied_fixes`, computed `stale`. |
| `feat/capture-runner` (`0cc308e`, `51a6e7e`) | `feat/fix-comparison-views` | Slice 1 (scenarios) and slice 2 (jobs), plus both design docs. **Work here.** |
| `fix/site-hero-ux-audit` (`a26b1eb`, `df02f1a`) | `develop` | Landing-page UI fixes, independent of the above (section 7). |

`feat/isolated-fix-pipeline` is an older local branch on a pre-refactor layout (`src/probe.rs`).
It was not part of this work and was not used. Look before reusing anything from it.

`rupicola-protocol` (`/home/lucasbrt/Projects/rupicola-protocol`, GitHub
`Lucas-BRT/rupicola-protocol`): PR #11 is merged into `develop` (`e9c748e`). It added: automatic
settle after each web action (the screenshot used to race scrolling), `Step::Checkpoint {
full_page }` with `Capability::FullPage` (web backend only), `UiAction::ScrollIntoView`,
`ScrollTo`, `ScrollToEnd`, `Environment.settle_ms`, and `Scroll` with a target no longer
hovering. Issues #8, #9, #10 are closed by it.

### Built and tested (231 tests pass on `feat/capture-runner`)

**Comparison views and staleness** (`feat/fix-comparison-views`)
- `crates/rupicola-api/src/fixes.rs`: `ComparisonView { Isolated, Integrated }`; `Comparison`
  gained `view`, `base_revision`, `revision`, `applied_fixes`, and `stale` (never stored,
  computed on read; clients' value ignored). `Comparison::is_stale(own, approved)`: only an
  integrated comparison can be stale, when its `applied_fixes` differ from the project's
  approved fixes other than the fix itself.
- Migration `0010_fix_comparison_views.sql`; `repositories/fixes.rs` reads and writes the new
  columns and computes `stale` in `load` and `list_details`.
- Only isolated comparisons feed a fix's Before/After selections
  (`services/fixes.rs::proofs`). An isolated comparison may not list `applied_fixes`
  (`domain/fix.rs`).
- Panel: `ComparisonPair::caption()` in `apps/panel/src/records.rs` shows view, revisions and
  "stale, recapture" on the comparison buttons. MCP `fix_create` schema takes the new fields.
- Existing fixes' comparisons read as `isolated` (migration default). **That is wrong for the 10
  fixes approved in the dev panel**: they were captured on a branch with all of them applied.
  They were not relabelled. They should be recaptured once the runner works.

**Scenarios** (slice 1)
- `crates/rupicola-api/src/scenarios.rs`: `Scenario`, `Viewport` (`"390x844@2"`), `Matrix`
  (`cells()` = viewports x themes, each at least 1), `ThemeSetup`, `Checks`, `ContrastLevel`,
  `Scenario::from_toml`, `Scenario::hash()` (SHA-256 of the canonical JSON, so reformatting
  does not change it). Unknown fields are an error (`deny_unknown_fields`).
- Server: migration `0011_scenarios.sql` (`scenario_versions`, keyed by project, name, hash;
  pushing an existing hash only makes it the latest), `domain/scenario.rs` (bounds; reuses
  `domain/capture_run.rs::check_steps` and `check_setup_steps`),
  `repositories/scenarios.rs`, `services/scenarios.rs`, `http/handlers/scenarios.rs`.
  Routes: `POST|GET /v1/projects/{id}/scenarios`, `GET .../{name}`, `GET .../{name}/{hash}`.
  MCP: `scenario_put`, `scenario_list`, `scenario_get`.
- CLI: `rupicola scenarios check|push|list|show` (`apps/cli/src/commands/remote/scenarios.rs`);
  default path `.rupicola/scenarios`. `.gitignore` no longer ignores `/.rupicola/scenarios/`.
- First real scenario: `.rupicola/scenarios/site-home.toml` (3 viewports x dark/light, full-page
  checkpoint, checks declared). It is pushed to the dev server, project `rupicola-site`.

**Jobs** (slice 2)
- `crates/rupicola-api/src/jobs.rs`: `JobStatus` (queued, running, done, failed, cancelled),
  `TreeSpec { name, base, merge, fixes }`, `NewJob`, `Job`, `JobCapture { capture, tree,
  viewport, theme }`, `JobProgress`, `JobResult`, `JobFailure`.
- Migration `0012_capture_jobs.sql` (`capture_jobs`, `capture_job_captures`).
  `domain/job.rs`: `LEASE_MS = 120_000`, ref validation (no leading `-`, no whitespace),
  1 to 8 trees.
- `repositories/jobs.rs`: `claim` is one `UPDATE ... RETURNING` taking the oldest queued job,
  or a running one whose lease expired (it restarts at 0 captures); `progress`, `complete`,
  `fail` only succeed for the credential that holds the job (`runner_key` = token digest,
  `Principal::key()`); `cancel`.
- Routes: `POST|GET /v1/projects/{id}/jobs` (`?open=true`), `POST .../jobs/claim` (204 when
  empty), `GET /v1/jobs/{id}`, `POST /v1/jobs/{id}/progress|complete|fail|cancel`. A cancelled
  job is returned (not an error) to the holder's next `progress`, so the runner stops.
  Devices may run jobs; only people may create or cancel them. A job's captures must belong to
  the project.
- MCP `job_create|list|get|cancel`; CLI `rupicola jobs create|list|show|cancel`
  (`--tree NAME=BASE[+MERGE...]`). `rupicola-client::Api` already has `jobs`, `job`,
  `create_job`, `claim_job`, `job_progress`, `complete_job`, `fail_job`, `cancel_job`.

### Not built

Everything that actually runs a job: the runner, comparison pairing and pixel difference,
automatic recapture and its setting, the panel indicator, the checks, and the protocol change for
style-based checks. A job created today stays `queued`.

## 4. Next steps

Do them in this order; commit each slice on its own (the user asked for atomic commits, push
stays separate and needs asking).

### Slice 3: the runner (`rupicola runner`)

A long-lived command in `apps/cli` that loops: claim, build trees, capture, upload, complete.

- **Auth:** the stored person token, or a project **device** token (`POST
  /v1/projects/{id}/devices`). Both can claim. Reuse the CLI's `Client`/`Scoped` flags.
- **Config: `.rupicola/project.toml`** in the repository (`reference/capture-scenarios.md`
  already names `.rupicola/project.toml`). Proposed, not yet confirmed by the user:
  ```toml
  [preview]
  command = "cargo run -q -p rupicola-site -- --port {port}"
  ready = "http://127.0.0.1:{port}/healthz"
  timeout_secs = 180
  [project]
  base = "develop"
  ```
- **Trees:** `git worktree add --detach <scratch>/<job>/<tree> <base>`, then `git merge
  --no-edit <ref>` for each `merge` entry. A conflict fails the job with a message. Record
  `git rev-parse --short HEAD` as the tree's revision. Always remove the worktrees, even on
  error. Use a scratch directory under the system temp dir and delete it; the user cares about
  SSD writes (see section 6).
- **Preview:** pick a free port, spawn the command in the worktree with `{port}` filled in, poll
  `ready` until it answers or the timeout passes, kill it afterwards. Keep the tail of its
  stderr to put in a failure message.
- **Heartbeat:** the lease is 120 s and a `cargo` build can take longer, so report progress
  (same `cells_done`) on a timer while building, not only after each capture. Stop when the
  reply says `cancelled`.
- **Matrix to captures:** for each viewport x theme build a `rupicola_capture::session::Plan`.
  Target = tree base URL + `scenario.url`. `Environment` from the viewport (size, scale), the
  scenario's `reduced_motion` and `locale`, `warmup_ms = 0`. A theme without setup steps sets
  `color_scheme` (`light`/`dark`); a theme with steps runs them first (after `ready`). Then the
  scenario's own steps.
- **Shared step mapping (needed):** the translation from the JSON step kinds (`CaptureStep`) to
  `rupicola_capture::session::Step` lives in `apps/server/src/domain/capture_run.rs`
  (`action_steps`, `step_into_plan`, `selector_target`). The runner needs the same code. Extract
  it into a library crate both depend on rather than duplicating it.
- **Capture:** `rupicola_capture::asynchronous::{WebBackend, run_plan}` (the workspace
  dependency already enables `async`). Interactive steps need `WebBackend`, which needs
  `RUPICOLA_PLAYWRIGHT_DIR`; `CdpBackend` only takes screenshots. Use `probe_environment()` to
  say what is missing. This stays inside the AGENTS.md capture boundary: `rupicola` consumes the
  protocol crate and implements no driver.
- **Upload:** reuse the capture-record path, not a new one: blobs through
  `Api::missing_blobs` and `Api::upload_project_blob`, then `Api::publish_capture` with a
  `rupicola_api::record::CaptureRecord` (`ScenarioRef { name, sha256: Some(scenario hash) }`
  carries the scenario version; set `revision` to the tree revision). Study
  `apps/cli/src/publish.rs` (manifest bundles) and `apps/server/src/services/publish.rs` (records)
  first. One capture per matrix cell.
- **Finish:** `complete_job` with one `JobCapture` per cell, or `fail_job` with a bounded error.
- **Determinism:** record the browser version. Capture the first tree's first cell twice; if the
  PNG hashes differ, flag the scenario unstable (a finding on the capture) before blaming a fix.
- **Tests:** a temp git repo for the worktree builder (merge, conflict, cleanup), the preview
  wait with a stub server, and the job loop against `rupicola-client`'s `mockall` mock (feature
  `mock`). Web tests that need Chromium return early without `RUPICOLA_PLAYWRIGHT_DIR`; set it
  or they prove nothing.
- **Acceptance:** with the dev server up, `rupicola scenarios push`, `rupicola jobs create
  site-home --tree base=develop --tree fixed=develop+fix/site-hero-ux-audit`, then
  `rupicola runner` drains it: the job reaches `done`, `cells_done` was 12 of 12 along the way,
  and 12 captures are listed on the job.

### Slice 4: comparison jobs, pairing, pixel difference, automatic recapture

- **Gap to close first: a fix has no git ref.** The server cannot say which branch to merge for
  an approved fix. Add `git_ref` to `NewFix`/`fixes` (migration, API, MCP schema). Also a project
  `base_ref` setting (default `develop`). Then the server can compose a fix's trees: `base`,
  `fix` (base + the fix's ref), `approved` (base + approved refs), `approved+fix`. With no
  approved fixes there are two trees and no integrated view.
- **Pairing, on `complete`:** for a job with a `fix`, pair captures by scenario, checkpoint id,
  viewport and theme: isolated = (`base`, `fix`), integrated = (`approved`, `approved+fix`). Store
  them as the fix's comparisons with `view`, `base_revision`, `revision`, `applied_fixes` (the
  approved fix ids in the tree), `route` = scenario url. Replace that fix's comparisons for the
  views the job produced, and regenerate the isolated Before/After selections.
- **Pixel difference:** decode both PNGs (there is no image crate in the workspace; adding one is
  justified here, mention it), compute `changed_ratio` with a per-channel tolerance and optional
  ignored regions, and store a diff PNG as the comparison's `diff` artifact.
- **Automatic recapture:** when a fix is approved (`services/fixes.rs::review`), enqueue a
  comparison job for every pending fix, recomputing `approved`. Skip if the fix already has an
  open job (cancel or replace). Add a setting (project and server, `services/settings.rs`) named
  something like `auto_recapture`, default on, honoured at enqueue time.
- **Acceptance:** two pending fixes A and B with isolated and integrated comparisons; approve A;
  B's integrated comparison is stale, a job for B is queued, the runner recaptures it, and the
  comparison is no longer stale with `applied_fixes = [A]`.

### Slice 5: panel

- A visible indicator that matrix captures are in progress: in the project header (open jobs and
  `cells_done`/`cells_total`) and on each fix card (which of its comparisons are being
  recaptured). `GET /v1/projects/{id}/jobs?open=true` exists. The panel is server-rendered
  Topcoat: add a small route that returns a fragment or JSON and poll it from the page.
- The `auto_recapture` toggle in settings.
- Keep the panel's own layout rules: modules route, markup lives in `views/`, logic in route
  modules (see section 6). After any panel change, check it at 1280, 820 and 390 px.

### Slice 6: checks

`Scenario.checks` is validated and stored but nothing runs it.

- **Doable now from the UI tree** the protocol returns (`UiSnapshot`: `bounds`, `role`, `label`,
  `state.visible/focused/pressed`, `style_hint` = class): `no_horizontal_overflow`,
  `min_target_size`, `no_overlap` (bounds of elements matching the selectors),
  `accessible_names`. Set `Environment.ui_tree` to `Full` for checked plans. Report failures as
  findings on the capture (`Api::add_finding`).
- **Needs a protocol change:** `contrast`, font size, "this block scrolls", `aria-selected`. The
  tree has no computed style. File an issue in `rupicola-protocol` in the existing format
  (Motivation, Minimal Context / Test Example, Suggested Fix; see issues #5 and #6) asking for an
  optional `style` on tree nodes (color, background, font size, overflow), then implement it in a
  PR to that repo's `develop` the way PR #11 was done, bump `Cargo.lock` here, and add the
  checks. AGENTS.md requires the issue for any capture limitation.
- The audit scripts used by hand in the earlier session (overflow, target size, contrast,
  accessible names, `aria-pressed`) are the specification of these checks; their results are in
  the dev panel's problems.

## 5. Gotchas that cost time

- **Local services** (not running for you; start them): from the repo root,
  `RUPICOLA_PLAYWRIGHT_DIR=$HOME/.local/share/mise/installs/node/lts/lib/node_modules/playwright
  RUPICOLA_ALLOW_LOCAL_CAPTURES=true target/debug/rupicola-server` (the bool needs `true`, not
  `1`; the default DB path `.rupicola/panel.db` is relative, so a server started from another
  directory makes a second empty database). Panel: `./apps/panel/dev.sh reload` (4731, backend
  4730). Site: `target/debug/rupicola-site` (4732). Seed: run `apps/server/seed-local.sql`
  against `.rupicola/panel.db` once the server has migrated it; login `demo` /
  `demo-password-1234`, projects `atelier-checkout`, `cosmic-notes`, `docs-portal`, plus
  `rupicola-site` created for this work.
- **Never `pkill -f rupicola-server`**: the pattern matches your own shell. Use
  `kill $(pgrep -x rupicola-server)`. Each Bash call is a fresh shell, so `export` does not
  persist between calls.
- **Two MCP servers exist.** `rupicola` points at production (`rupicola.lucasbrt.dev`); do not
  use it for local work. `rupicola-local` points at `127.0.0.1:4730` with a token in
  `.rupicola/local-mcp` (a scratch `XDG_DATA_HOME`, so the real token is untouched). It was added
  with `claude mcp add rupicola-local -s local`. To use the CLI against the dev server:
  `XDG_DATA_HOME=$PWD/.rupicola/local-mcp target/debug/rupicola ...`. A schema change needs a
  Claude Code restart before the agent sees new MCP tool parameters.
- **sqlx takes only literal SQL.** `format!` for a query fails to compile; use a `macro_rules!`
  with `concat!` (see `repositories/jobs.rs`) or `QueryBuilder`.
- **Tests that need a browser pass vacuously without `RUPICOLA_PLAYWRIGHT_DIR`.** This hid a
  broken test twice in the protocol work. Mutate the code once to confirm a test can fail.
- **`apps/site` CSS layers.** `@layer base, layout, components`: a later layer wins whatever the
  specificity. `.btn { display: inline-flex }` (components) beat `.nav-toggle` and
  `.topbar-panel-cta` (layout), so the Menu button showed on desktop and Open Panel never hid on
  phones. Rules that override `.btn` must live in `components`, after it.
- **`rupicola-api` needs `toml`** for `Scenario::from_toml`; `Cargo.lock` changed for it (the
  only reason the lock file was touched).
- A capture's `color_scheme` is ignored by a page that pins its own theme (the landing page
  does); drive such themes with steps.

## 6. Rules to follow

From `AGENTS.md`: Conventional Commits (a hook checks them); `develop` -> PR -> `main`, never push
to `main`; before reporting done run and observe `cargo fmt --check`, `cargo clippy --locked
--workspace --all-targets -- -D warnings`, `cargo nextest run --locked --workspace` and `cargo
run --locked -- check .` (set `RUPICOLA_PLAYWRIGHT_DIR` or say that web tests were skipped); no
captures, browsers or drivers belong in this repo, only consuming `rupicola-protocol`; open a
GitHub issue on `Lucas-BRT/rupicola-protocol` for any capture limitation; do not install local
systemd services; do not add dependencies or change `Cargo.lock` unless needed; prototype freedom
allows breaking the schema.

From the user's standing preferences: no bare literals (named constants, messages built from
them); behaviour lives on its type, conversions through `FromStr`/`Display`/`From`; backend code
uses `anyhow`, `thiserror`, `tracing`, `tower-http`, never `Box<dyn Error>` or `println!` for
logging (the CLI prints results with `println!`, as the existing commands do); everything is
async (`tokio`, `reqwest`, `tokio::process`), no sync-only crates; commit each verified change on
its own, do not push without asking; panel markup only in `views/`, routing by module tree; check
any frontend change at 1280, 820 and 390 px; throwaway output goes to scratch and is deleted.

The user writes in Portuguese and wants honest reports: say what was not verified, never claim a
check that was not run.

## 7. Parallel track: the landing-page UI/UX cleanup

Branch `fix/site-hero-ux-audit` (not pushed, no PR; the user wants the PR only after the UI/UX is
completely clean). In the dev panel's `rupicola-site` project there are 12 problems from an
audit of the home page against every module in `reference/ux/`: 10 fixes approved by the user,
one fix (`Hide the header CTA on phones and left-align the install command`) pending review, and
one problem left open on purpose ("Site copy is hard-coded in views with no string catalog"),
which needs the user to decide whether the site will be localized.

Still to do there: audit `/install`, `/docs` and `/architecture` the same way (only `/` was
audited), the hero demo's AFTER card is still partly hidden by the split handle at 50% (that is
the slider revealing, not a collision, but it reads badly on a phone), and at 390 px with 200%
text the header still overflows by 43 px. Once the runner exists, redo these fixes' evidence in
isolation (each fix on its own branch from `develop`), which is the whole point of this work.

## 8. Open questions for the user

- Preview config in `.rupicola/project.toml` as proposed, or elsewhere?
- Fix `git_ref` and project `base_ref`: server-side fields as proposed in slice 4, or should the
  runner resolve `@base` / `@approved` symbolically?
- Should the in-process runner for a locally installed server be part of slice 3, or after?
- Job expiry: nothing removes a queued job nobody takes, and the 120 s lease is a guess.
- Retention of capture matrices: a recapture after every approval creates many captures; decide
  what is kept (the latest per fix and view?) so the blob store does not grow without bound.
