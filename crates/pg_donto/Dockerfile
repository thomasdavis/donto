# Build + test the pg_donto extension end-to-end inside a container.
# Used by `scripts/pgrx-build.sh` so contributors without host pg-dev
# headers can verify the extension. Also used by the `ext` CI job.
#
# Build:
#   docker build -t donto-pgrx -f crates/pg_donto/Dockerfile .
# Test:
#   docker run --rm donto-pgrx
#
# Both are wrapped by scripts/pgrx-build.sh.

FROM rust:1-slim-bookworm

ARG PG_VERSION=16

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config libclang-dev clang \
        libssl-dev libreadline-dev zlib1g-dev libxml2-dev \
        flex bison \
        gnupg ca-certificates curl wget lsb-release sudo git \
    && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
        | gpg --dearmor -o /etc/apt/trusted.gpg.d/pgdg.gpg \
    && echo "deb https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" \
        > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        postgresql-${PG_VERSION} \
        postgresql-server-dev-${PG_VERSION} \
        libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# pgrx needs a non-root user (it manages a PG instance under $HOME).
RUN useradd -m -s /bin/bash builder \
    && echo "builder ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/builder

# `cargo pgrx install` writes the .control + .sql + .so into the system
# PG install dirs. The builder user needs write access so we don't have
# to sprinkle `sudo` through the CMD. These are ephemeral container dirs
# so loosening perms is fine.
RUN chown -R builder:builder \
      /usr/share/postgresql/${PG_VERSION}/extension \
      /usr/lib/postgresql/${PG_VERSION}/lib

USER builder
WORKDIR /home/builder

ENV CARGO_HOME=/home/builder/.cargo
ENV PATH="/home/builder/.cargo/bin:${PATH}"
# pgrx-tests reads $USER to name its test pg role; the base rust:slim
# image doesn't set it, so wire it explicitly.
ENV USER=builder

# sccache: a build cache for Rust. Speeds up repeat `cargo pgrx package`
# calls (used by CI matrix across pg versions). Installed as a prebuilt
# musl binary so we don't spend 5 minutes compiling it from source. We
# turn it on for *our* crate build only; `cargo install cargo-pgrx`
# runs first and can compile without it.
USER root
RUN curl -fsSL \
      https://github.com/mozilla/sccache/releases/download/v0.8.2/sccache-v0.8.2-x86_64-unknown-linux-musl.tar.gz \
      | tar xz -C /tmp \
    && install -m 0755 /tmp/sccache-v0.8.2-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
    && rm -rf /tmp/sccache-v0.8.2-x86_64-unknown-linux-musl
USER builder
ENV SCCACHE_DIR=/home/builder/.cache/sccache

RUN cargo install --locked cargo-pgrx --version 0.12.7

# Initialize pgrx against the system PG install rather than letting it
# download its own.
RUN cargo pgrx init --pg${PG_VERSION} $(which pg_config)

# Copy only what the extension build needs.
COPY --chown=builder:builder sql/ /home/builder/donto/sql/
COPY --chown=builder:builder crates/pg_donto/ /home/builder/donto/crates/pg_donto/

WORKDIR /home/builder/donto/crates/pg_donto

# Default: build, install into the test instance, run the #[pg_test] suite.
# RUSTC_WRAPPER=sccache is scoped to this invocation so cargo-pgrx's own
# compilation (above) isn't affected.
CMD ["sh", "-c", "RUSTC_WRAPPER=sccache cargo pgrx package --features pg${PG_VERSION:-16} --no-default-features && RUSTC_WRAPPER=sccache cargo pgrx test pg${PG_VERSION:-16} --features pg${PG_VERSION:-16} --no-default-features && sccache --show-stats"]
