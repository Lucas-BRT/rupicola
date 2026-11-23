# Rupicola panel

Review screenshots, recordings and analysis by project. Topcoat serves the
routes and renders the pages. Tailwind compiles the panel stylesheet at build
time, and the button, badge and card components come from Topcoat UI 0.11.
Turso stores the index, accounts and access records. Captures stay on disk.

## Local

Install with Rust 1.99 or newer:

```sh
cargo install --path apps/panel --locked
rupicola serve --server-url http://127.0.0.1:4730
```

Go to http://127.0.0.1:4731 and create the owner account on first access.
Captures, their media and their review notes live in the backend: publish them with
`rupicola capture --server-url …` or `rupicola publish`, and the panel reads them from there.
The panel's own database (`--db`) now only holds design-system versions. Running `rupicola design open` in a checkout registers it before its first capture. The project’s **Design** page shows the brief, art direction, tactics and decisions recorded by the CLI. Use **Projects → Create a project** to prepare a
project before its first upload.

Tailwind's standalone compiler is fetched at build time. Set `TAILWIND_CLI` to
a trusted, preinstalled 4.3.2 executable for offline builds. The compiled
stylesheet is embedded in the binary.

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
  --db /srv/rupicola/panel.db \
  --server-url http://127.0.0.1:4730 \
  --port 4731
```

The upstream binds to loopback. Terminate TLS at the proxy and keep the upstream
private. Back up the panel's data directory and the backend's database and media
store together. Accounts, memberships, access records, captures and their media
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
rupicola capture scenario.toml --server-url https://atelier.example.org
rupicola publish path/to/capture --server-url https://atelier.example.org
```

Both use the token stored by `rupicola auth set-token`. Uploaded runs start with inspection pending.
