# syntax=docker/dockerfile:1

# Lanobe builds in three stages so the shipped image carries neither the Rust
# toolchain nor node_modules: a ~20 MB binary on a slim Debian base. That matters
# because the deployment target is a t2.micro with 1 GB of RAM.

# ---- 1. WebUI -----------------------------------------------------------------
FROM node:22-bookworm-slim AS webui

WORKDIR /webui

# Dependencies first: this layer is only invalidated when the lockfile changes.
COPY WebUI/package.json WebUI/yarn.lock ./
RUN yarn install --frozen-lockfile --network-timeout 600000

COPY WebUI/ ./

# `yarn build` runs `yarn setup` first, which reinstalls and copies .env.template
# into place. Neither is wanted here: dependencies are already installed in the
# layer above, and the two variables the vite config reads are set directly, so
# the build does not depend on a dotfile being present.
ENV PORT=3000 \
    ALLOWED_HOSTS=""
RUN npx vite build

# ---- 2. Server ----------------------------------------------------------------
# Pinned: CI, Docker and local builds must agree on the toolchain. A floating
# tag means clippy gains lints between builds and `-D warnings` fails on code
# that was clean when written.
FROM rust:1.95-bookworm AS server

# libsqlite3-sys, zstd-sys and ring all compile C, so a toolchain is required
# even though the binary itself links no system OpenSSL (reqwest uses rustls).
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY bin/ bin/
COPY crates/ crates/

# The frontend is embedded at compile time via rust-embed.
COPY --from=webui /webui/build/ bin/lanobe/resources/webui/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release && cp target/release/lanobe /usr/local/bin/lanobe

# ---- 3. Runtime ---------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 lanobe

COPY --from=server /usr/local/bin/lanobe /usr/local/bin/lanobe

# Data (databases) and the EPUB library are both mounted; nothing is written
# inside the image itself.
ENV LANOBE_HOST=0.0.0.0 \
    LANOBE_PORT=4567 \
    LANOBE_DATA_DIR=/data \
    LANOBE_LIBRARY_PATH=/data/library

RUN mkdir -p /data/library && chown -R 1000:1000 /data
VOLUME ["/data"]

USER 1000
EXPOSE 4567

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${LANOBE_PORT}/api/system/version" || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/lanobe"]
