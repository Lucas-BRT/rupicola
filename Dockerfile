# syntax=docker/dockerfile:1
#
# One Dockerfile, three images (select with `--target`):
#   * `site`    — lean landing & documentation container (no ffmpeg, no browser);
#   * `backend` — API server, the only process that touches the database and media store;
#   * `panel`   — panel + CLI with ffmpeg and the Design review browser (default, last stage).
#
# Multi-stage and cache-friendly:
#   * cargo-chef is installed in its own stage with a pinned version and rebuilds only when that pin changes;
#   * cargo-chef cooks workspace dependencies from recipe.json, so editing source code never recompiles them;
#   * one builder compiles the backend, panel, CLI and site with the `website` profile, so both images share its cache;
#   * runtimes run as non-root uid 10001 with a self-contained --healthcheck probe.

FROM rust:1.99.1-bookworm AS base
WORKDIR /app

# ---- tools -------------------------------------------------------------------------------------------------
FROM base AS tool-chef
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install cargo-chef --version 0.1.77 --locked

# ---- recipe: changes only when manifests or Cargo.lock change ----------------------------------------------
FROM tool-chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- panel + cli + site build ------------------------------------------------------------------------------
FROM tool-chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo chef cook --profile website --locked -p rupicola -p rupicola-backend -p rupicola-panel -p rupicola-site --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --profile website --locked -p rupicola -p rupicola-backend -p rupicola-panel -p rupicola-site

# ---- backend runtime ---------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS backend
RUN useradd --system --no-create-home --uid 10001 rupicola

WORKDIR /app
COPY --from=builder /app/target/website/rupicola-backend ./rupicola-backend

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4730

EXPOSE 4730
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["./rupicola-backend", "--healthcheck"]
CMD ["./rupicola-backend"]

# ---- site runtime ------------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS site
RUN useradd --system --no-create-home --uid 10001 rupicola

WORKDIR /app
COPY --from=builder /app/target/website/rupicola-site ./rupicola-site

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4732

EXPOSE 4732
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["./rupicola-site", "--healthcheck"]
CMD ["./rupicola-site"]

# ---- browser used by the Design review gate ----------------------------------------------------------------
FROM node:24.22.0-bookworm-slim AS design-browser
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/rupicola-browsers
RUN npm install --prefix /opt/rupicola-validator --no-audit --no-fund --no-package-lock playwright-core@1.64.1 \
    && node /opt/rupicola-validator/node_modules/playwright-core/cli.js install --only-shell chromium

# ---- runtime -----------------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS runtime-base
COPY --from=design-browser /usr/local/bin/node /usr/local/bin/node
COPY --from=design-browser /opt/rupicola-validator /opt/rupicola-validator
COPY --from=design-browser /opt/rupicola-browsers /opt/rupicola-browsers
RUN node /opt/rupicola-validator/node_modules/playwright-core/cli.js install-deps chromium \
    && apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates \
      ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 rupicola \
    && mkdir -p /data/captures /data/uploads /data/design /data/proposals /data/problems /data/fixes \
    && chown -R 10001:10001 /data \
    && chmod 0700 /data

FROM runtime-base AS panel
WORKDIR /app
COPY --from=builder /app/target/website/rupicola-panel ./rupicola-panel
COPY --from=builder /app/target/website/rupicola ./rupicola

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4731 \
    XDG_DATA_HOME=/data \
    RUPICOLA_DB=/data/panel.db \
    RUPICOLA_CAPTURES=/data/captures \
    RUPICOLA_UPLOADS=/data/uploads \
    RUPICOLA_PLAYWRIGHT_MODULE=/opt/rupicola-validator/node_modules/playwright-core \
    PLAYWRIGHT_BROWSERS_PATH=/opt/rupicola-browsers

EXPOSE 4731
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["./rupicola-panel", "--healthcheck"]
CMD ["./rupicola-panel"]
