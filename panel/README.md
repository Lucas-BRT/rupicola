# Ferris Atelier panel

Review screenshots, recordings and analysis by project. Topcoat serves the
routes and renders the pages. Tailwind compiles the panel stylesheet at build
time, and the button, badge and card components come from Topcoat UI 0.11.
Turso stores the index, accounts and access records. Captures stay on disk.

## Local

Install with Rust 1.99 or newer:

```sh
cargo install --path panel --locked
ferris-atelier serve --captures /path/to/project/.ferris-atelier/captures
```

Go to http://127.0.0.1:4731 and create the owner account on first access.
Repeat `--captures` to use several directories. Each directory contains runs
with version 1 manifests and their relative artifacts. Projects appear as the
panel indexes their manifests. Running `ferris-atelier design open` in a checkout registers it before its first capture. The project’s **Design** page shows the brief, art direction, tactics and decisions recorded by the CLI. Use **Projects → Create a project** to prepare a
project before its first upload. Keep the order of capture roots stable to
preserve links to runs.

Tailwind's standalone compiler is fetched at build time. Set `TAILWIND_CLI` to
a trusted, preinstalled 4.3.2 executable for offline builds. The compiled
stylesheet is embedded in the binary.

## Shared server

Run one panel instance behind an HTTPS reverse proxy on the same host. Mount
private persistent storage for the database and uploads. Run under a dedicated
service user that owns those directories, with a restrictive file mode and
access to only the capture roots it needs. Configure the proxy to preserve the
public Host header, allow requests up to 64 MiB and rate-limit sign-in and
registration.

Before first startup, provision `FERRIS_SETUP_CODE` as a high-entropy secret
of at least 32 characters. Enter it in the setup form. After the owner account
exists, remove the environment secret from the service configuration.

```sh
ferris-atelier serve \
  --public-origin https://atelier.example.org \
  --db /srv/ferris/panel.db \
  --uploads /srv/ferris/captures \
  --port 4731
```

The upstream binds to loopback. Terminate TLS at the proxy and keep the upstream
private. Back up the Turso database and capture directories together. The run
index can be rebuilt. Accounts, memberships and access records live in the
database. Machine uploads are stored on the host filesystem.

In **Project settings**, create a Reader or Reviewer invitation. Share its code
and the registration URL through a private channel. Codes expire after 24 hours.
Readers can open the project's evidence. Reviewers can also record inspection
notes. The owner manages projects, access and machine credentials. A member with an account can redeem a new invitation under **Settings → Join a project**.

For password recovery, stop the panel service and reset the account from the host:

```sh
ferris-atelier-panel --db /srv/ferris/panel.db recover USERNAME
```

Pass the replacement password on standard input without shell echo. This
revokes all sessions for the account. Restart the panel afterward. Signed-in users can also rotate their own password directly in **Settings → Account → Change password**, which verifies the current password with Argon2id and revokes all other active sessions.

## Docker & Dokploy (Open Internet)

From the workspace root:

```sh
# Production behind Dokploy's Traefik (uses expose: 4731 on dokploy-network)
FERRIS_PUBLIC_ORIGIN=https://atelier.example.org \
FERRIS_SETUP_CODE="$(openssl rand -hex 32)" \
docker compose up -d --build

# Local testing on http://127.0.0.1:4731
docker compose -f compose.local.yml up -d --build
```

The container runs as non-root `10001:10001` with a read-only root filesystem, `cap_drop: [ALL]`, `no-new-privileges:true`, and a built-in `./ferris-atelier-panel --healthcheck` probe against `/healthz`.

## Publish from another machine

Create a publishing credential in **Project settings → Machines**. Store it
securely as `FERRIS_PUBLISH_TOKEN` on the capture machine and run:

```sh
ferris-atelier-panel publish path/to/capture --server https://atelier.example.org
```

The publisher requires curl. It sends the manifest and only its declared
artifacts, with a limit of 40 MiB of media per run. The destination must match
the manifest's project. Identical retries reuse the run. Uploaded runs start
with inspection pending. Revoking a credential prevents subsequent uploads.
