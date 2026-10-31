# syntax=docker/dockerfile:1.7
# anneal's production image: the API (which also serves the built web app), the `anneal` CLI, the content, and the
# Rust toolchain rust-analyzer needs for the editor. Submitted code never runs here: the API starts the separate
# runner image (docker/runner.Dockerfile) through the host's Docker socket. See docs/DEPLOY.md.

# ---- web app ----
FROM node:24-slim AS web
RUN corepack enable && corepack prepare pnpm@9.15.4 --activate
WORKDIR /web
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# ---- Rust: dependencies first (cargo-chef), so a code-only change reuses the cached dependency layer ----
FROM rust:1.98-slim AS chef
RUN cargo install cargo-chef --locked --quiet
WORKDIR /src

FROM chef AS plan
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=plan /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p anneal-api -p anneal-cli
COPY . .
RUN cargo build --release --locked -p anneal-api -p anneal-cli \
    && cp target/release/anneal-api target/release/anneal /usr/local/bin/

# ---- the meaning model (bge-small-en-v1.5): pinned to one revision and checked, the same files as tools/fetch-model.sh ----
FROM scratch AS model
ADD --chmod=644 --checksum=sha256:094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750 https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/5c38ec7c405ec4b44b94cc5a9bb96e735b38267a/config.json /config.json
ADD --chmod=644 --checksum=sha256:d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66 https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/5c38ec7c405ec4b44b94cc5a9bb96e735b38267a/tokenizer.json /tokenizer.json
ADD --chmod=644 --checksum=sha256:3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/5c38ec7c405ec4b44b94cc5a9bb96e735b38267a/model.safetensors /model.safetensors

# ---- runtime ----
FROM rust:1.98-slim
# rust-analyzer checks the editor buffer (cargo check, or clippy with Live clippy) and /api/format runs rustfmt,
# so the runtime keeps the toolchain.
RUN rustup component add rust-analyzer rust-src rustfmt clippy \
    && rm -rf /usr/local/rustup/downloads /usr/local/rustup/tmp
# Only the Docker CLI: the daemon is the host's, reached through the mounted socket.
COPY --from=docker:29-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=build /usr/local/bin/anneal-api /usr/local/bin/anneal /usr/local/bin/
COPY content /app/content
# Search by meaning embeds the query text on this machine; nothing leaves it (crates/api/src/meaning.rs).
COPY --from=model / /app/models/bge-small-en-v1.5
# Course definitions for the Courses pages. The reference solution is not in git and is excluded by .dockerignore.
COPY courses /app/courses
COPY --from=web /web/dist /app/web/dist
# The CLI built for macOS and Linux by CI (empty in a local build): /install.sh downloads from here.
COPY dist-cli /app/downloads

WORKDIR /app
ENV ANNEAL_ADDR=0.0.0.0:8787 \
    ANNEAL_CONTENT=/app/content \
    ANNEAL_COURSES=/app/courses \
    ANNEAL_WEB_DIST=/app/web/dist \
    ANNEAL_DOWNLOADS=/app/downloads \
    ANNEAL_MODEL=/app/models/bge-small-en-v1.5 \
    ANNEAL_SANDBOX=docker \
    ANNEAL_DOCKER_CONTEXT= \
    ANNEAL_COOKIE_SECURE=true \
    RUST_LOG=info
EXPOSE 8787
CMD ["anneal-api"]
