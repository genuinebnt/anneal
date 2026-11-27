# Sandbox image for anneal-runner: stable rustc + clippy, plus the vendored crate set that
# problems may depend on (docker/deps). Runs are started with --network none, a read-only root
# filesystem and an unprivileged user, so cargo's home lives on a tmpfs and crates come from
# /opt/anneal-vendor. The network is only used here, at build time, to download that set.
FROM rust:1.98-slim

# Python for the practice tracks: tests run with the system interpreter, nothing is installed with pip.
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy \
    && rm -rf /usr/local/rustup/downloads /usr/local/rustup/tmp

# Vendor the crate set (docker/deps/Cargo.toml, pinned by its Cargo.lock).
COPY deps /opt/anneal-deps
RUN cd /opt/anneal-deps \
    && CARGO_HOME=/tmp/vendor-home cargo vendor --locked --quiet /opt/anneal-vendor > /dev/null \
    && rm -rf /tmp/vendor-home \
    && chmod -R a+rX /opt/anneal-vendor

ENV CARGO_HOME=/tmp/cargo-home \
    CARGO_TERM_COLOR=always \
    CARGO_INCREMENTAL=0 \
    RUSTC_BOOTSTRAP=1

WORKDIR /work
