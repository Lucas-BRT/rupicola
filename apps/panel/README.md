# Rupicola panel

Review screenshots, recordings and analysis by project. Topcoat serves the
routes and renders the pages. Tailwind compiles the panel stylesheet at build
time, and the button, badge and card components come from Topcoat UI 0.11.
The backend owns accounts, captures, problems, fixes, design records and design
systems in SQLite and its media store; the panel is stateless.

## Local

Install with Rust 1.99 or newer:

```sh
cargo install --path apps/panel --locked
rupicola serve --server-url http://127.0.0.1:4730
```

Go to http://127.0.0.1:4731 and create the owner account on first access.
Captures, their media and their review notes live in the backend: publish them with
`rupicola publish`, and the panel reads them from there.
The panel keeps no data of its own: accounts, projects, captures, problems, fixes, design records and design systems live in the backend. The project’s **Design** page shows the brief, art direction, tactics and decisions agents recorded in the backend. Use **Projects → Create a project** to prepare a
project before its first upload.

Tailwind's standalone compiler is fetched at build time. Set `TAILWIND_CLI` to
a trusted, preinstalled 4.3.2 executable for offline builds. The compiled
stylesheet is embedded in the binary.

## Source layout

Routes follow the module tree (topcoat module routing): `src/app/projects/project/problems.rs`
serves `/projects/{project}/problems`, and its child modules are that tab's actions. A route
module loads what its page shows and calls a view. Markup lives only in `src/views/`, which
takes loaded data and calls no backend. `auth/`, `project.rs`, `records.rs`, `runs.rs` and
`media.rs` read from the backend; `systems.rs` reads the design systems the backend validates and stores.
Pages sit under layouts (`app/_workspace.rs` for the document and navigation, `projects/project.rs` for the heading and tabs) and set their title with `Metadata`; backend reads shared by a request are `#[memoize]`d in `backend.rs`.

Topcoat's streaming (`suspense`, `live!`) and runtime (shards, signals) inject inline scripts, which the panel's Content-Security-Policy refuses, so the panel does not use them; `error_boundary` needs no script and is used.

The same layout applies to `apps/site` (`app/`, `views/`, `content/`, `assets/`).

## Shared server

Run one panel instance behind an HTTPS reverse proxy on the same host. Mount
private persistent storage for the database. Run under a dedicated
service user that owns those directories, with a restrictive file mode and
access to only the capture roots it needs. Configure the proxy to preserve the
public Host header, allow requests up to 64 MiB and rate-limit sign-in and
registration.

The panel keeps no accounts of its own: it asks the Rupicola backend, whose address
you give with `--server-url` (or `RUPICOLA_SERVER_URL`). The backend creates the first
administrator from the setup form; provision its `RUPICOLA_SETUP_CODE` as a high-entropy
secret of at least 32 characters before first startup and remove it afterwards.

```sh
rupicola serve \
  --public-origin https://atelier.example.org \
  --server-url http://127.0.0.1:4730 \
  --port 4731
```

The upstream binds to loopback. Terminate TLS at the proxy and keep the upstream
private. Back up the backend's database and media store; the panel itself is
stateless. Accounts, memberships, access records, captures and their media
live in the backend.

In **Project settings**, create a Reader or Reviewer invitation. Share its code
and the registration URL through a private channel. Codes expire after 24 hours.
Readers can open the project's evidence. Reviewers can also record inspection
notes. The owner manages projects and access. A member with an account can redeem a new invitation under **Settings → Join a project**.

Signed-in users can rotate their own password in **Settings → Account → Change password**, which verifies the current password and ends all other sessions. The backend owns the accounts, so a lost password is reset there.

## Docker & Dokploy (Open Internet)

From the workspace root:

```sh
# Production behind Dokploy's Traefik (uses expose: 4731 on dokploy-network)
RUPICOLA_PUBLIC_ORIGIN=https://atelier.example.org \
RUPICOLA_DOMAIN=atelier.example.org \
RUPICOLA_SETUP_CODE="$(openssl rand -hex 32)" \
docker compose up -d --build

# Local testing on http://127.0.0.1:4731
docker compose -f compose.local.yml up -d --build
```

The container runs as non-root `10001:10001` with a read-only root filesystem, `cap_drop: [ALL]`, `no-new-privileges:true`, and a built-in `./rupicola-panel --healthcheck` probe against `/healthz`.

## Publish from another machine

The panel does not take uploads. Publish captures to the backend with the CLI:

```sh
rupicola publish path/to/capture --server-url https://atelier.example.org
```

It uses the token stored by `rupicola login`. Uploaded runs start with inspection pending.
