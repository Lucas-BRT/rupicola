# syntax=docker/dockerfile:1
#
# Multi-stage and cache-friendly:
#   * cargo-chef is installed in its own stage with a pinned version and rebuilds only when that pin changes;
#   * cargo-chef cooks workspace dependencies from recipe.json, so editing source code never recompiles them;
#   * the panel and CLI build with the `website` profile (fat LTO, 1 codegen unit, stripped);
#   * runtime runs as non-root uid 10001 with a self-contained --healthcheck probe.

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

# ---- panel + cli build -------------------------------------------------------------------------------------
FROM tool-chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo chef cook --profile website --locked -p rupicola -p rupicola-panel --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --profile website --locked -p rupicola -p rupicola-panel

# ---- runtime -----------------------------------------------------------------------------------------------
FROM debian:bookworm-20261104-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates \
      ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 rupicola \
    && mkdir -p /data/captures /data/uploads /data/design /data/proposals /data/problems /data/fixes \
    && chown -R 10001:10001 /data \
    && chmod 0700 /data

WORKDIR /app
COPY --from=builder /app/target/website/rupicola-panel ./rupicola-panel
COPY --from=builder /app/target/website/rupicola ./rupicola

USER 10001:10001
ENV HOST=0.0.0.0 \
    PORT=4731 \
    XDG_DATA_HOME=/data \
    RUPICOLA_DB=/data/panel.db \
    RUPICOLA_CAPTURES=/data/captures \
    RUPICOLA_UPLOADS=/data/uploads

EXPOSE 4731
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["./rupicola-panel", "--healthcheck"]
CMD ["./rupicola-panel"]
