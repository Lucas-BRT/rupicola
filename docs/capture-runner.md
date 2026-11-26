# Scenarios, runners and fix comparisons

Status: design, agreed in conversation; slices land one at a time on `feat/capture-runner`.
Slices 1 and 2 are done. For the current state, decisions, gotchas and what to build next, read
`capture-runner-handoff.md`.

## Why

A fix's before/after was captured on whatever tree the author had at hand, so approving one
fix changed what another fix's comparison showed. Captures were also written by an agent each
time, which costs tokens and varies from run to run. This design makes a capture a file the
repository owns, makes running it a job anyone can request, and says which tree each
comparison was taken on.

## Pieces

**Scenario definition.** A TOML file under `.rupicola/scenarios/` in the repository. The
repository is the source of truth; the server only holds copies that were pushed
(`rupicola scenarios push`), content-addressed by the hash of the parsed definition, so a
formatting change does not make a new version. A definition holds the page, the environment,
a matrix of viewports and themes, steps (the same kinds `capture_run` takes) and checks. A
capture records the hash it came from, so repeating it is running that hash on another tree.

**Runner.** `rupicola runner` is a process on a machine that has the code. It authenticates as
a capture device, claims jobs from the server, builds the trees a job names, captures with
`rupicola-capture`, runs the checks, and uploads the result through the existing publish path.
The photographer uses Chromium CDP only; it never falls back to Playwright. Set
`RUPICOLA_CHROMIUM_BIN` or pass `--chromium-bin` when Chromium is not discoverable on `PATH`.
The server never runs project commands, so the same design works installed on a laptop, in
Docker, or on a cloud host that cannot reach a developer's `127.0.0.1`. On a laptop the server
can start one runner in-process.

**Job.** A request to run a scenario hash on one or more trees: queued, running with
`cells_done` of `cells_total`, then done or failed. A tree is `{revision, applied fixes}`. The
runner builds it from a git ref and the project's preview command, applying the fixes' branches.
MCP, the CLI and the panel all create jobs through the same API.

**Comparison.** For a fix, a job captures four trees: the base, the base plus the fix alone
(isolated), the base plus every approved fix, and that plus the fix (integrated). The server
pairs captures by scenario, checkpoint, viewport and theme, stores the pairs as the fix's
comparisons with their revisions, and computes the pixel difference. An integrated comparison
is stale once the approved fixes change (already implemented).

**Recapture.** When a fix is approved the server queues a comparison job for every pending fix
with an integrated comparison. This is on by default and a server setting turns it off. The
panel shows the queued and running jobs, with the cells done, wherever the fix's comparisons
appear.

## Definition

```toml
name = "home"
url = "/"                          # relative to the tree's base URL

[environment]
reduced_motion = true
locale = "en-US"

[matrix]
viewports = ["390x844@2", "820x1180", "1280x800"]   # WIDTHxHEIGHT, optional @SCALE
themes = ["dark", "light"]

# A theme with no steps sets the browser's color scheme. A theme with steps runs them
# once the page is ready, for sites that choose the theme themselves.
[themes.light]
steps = [{ kind = "click", selector = "[data-theme-toggle]" }]

[[steps]]
kind = "ready"
selector = "h1"

[[steps]]
kind = "checkpoint"
id = "full"
full_page = true

[checks]
no_horizontal_overflow = true
min_target_size = 24
no_overlap = [".diff-side > div"]
accessible_names = true
contrast = "aa"
```

## Checks

A check runs on the page the runner captured and is reported as a finding on the capture.
Some work from the UI tree the protocol already returns: `no_horizontal_overflow`,
`min_target_size`, `no_overlap` (bounds) and `accessible_names` (roles and labels). `contrast`,
font size and "this block scrolls" need computed style, which the tree does not carry yet; they
need a field in `rupicola-protocol`, filed there first.

## Determinism

The definition fixes viewport, scale, locale and reduced motion. The runner records the
browser version and captures the base tree twice; if the two differ the scenario is flagged
unstable before any fix is blamed. Pixel comparison takes a tolerance and ignored regions.

## Slices

1. Scenario definitions: types, TOML parsing, validation, hash, storage, API, MCP, CLI push.
2. Jobs: table, lifecycle and progress; runner claim, report and upload endpoints.
3. The runner: worktrees, preview, matrix expansion, capture, upload.
4. Comparison jobs, pairing, pixel difference and automatic recapture with its setting.
5. Panel: job indicator and the setting.
6. Checks: tree-based first, then the protocol change for style-based ones.
