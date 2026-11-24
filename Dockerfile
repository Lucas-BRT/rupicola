# syntax=docker/dockerfile:1
#
# One Dockerfile, three images (select with `--target`):
#   * `site`   — lean landing & documentation container;
#   * `server` — API server, the only process that touches the database and media store;
#   * `panel`  — stateless Topcoat web client talking to `server` (default, last stage).
#
# Multi-stage and cache-friendly:
#   * cargo-chef is installed in its own stage with a pinned version and rebuilds only when that pin changes;
#   * cargo-chef cooks workspace dependencies from recipe.json, so editing source code never recompiles them;
#   * one builder compiles server, panel and site with the `website` profile, so all images share its cache;
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

# ---- server + panel + site build ---------------------------------------------------------------------------
FROM tool-chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo chef cook --profile website --locked -p rupicola-server -p rupicola-panel -p rupicola-site --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --profile website --locked -p rupicola-server -p rupicola-panel -p rupicola-site

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

# ---- server runtime ----------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS server
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 rupicola

WORKDIR /app
COPY --from=builder /app/target/website/rupicola-server ./rupicola-server
RUN mkdir -p /data \
    && chown 10001:10001 /data \
    && chmod 0700 /data

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4730 \
    RUPICOLA_DB=/data/backend.db

EXPOSE 4730
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["./rupicola-server", "--healthcheck"]
CMD ["./rupicola-server"]

# ---- panel runtime -----------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS panel
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 rupicola

WORKDIR /app
COPY --from=builder /app/target/website/rupicola-panel ./rupicola-panel

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4731

EXPOSE 4731
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["./rupicola-panel", "--healthcheck"]
CMD ["./rupicola-panel"]
