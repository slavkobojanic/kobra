# syntax=docker/dockerfile:1
#
# Two stages: the K toolchain checks and builds the app, and nginx serves the
# static files it produces. Kanban renders on the client and has no
# server-only code, so `k build` writes only dist/public: there is no
# dist/app to run. See docs/design/38-deployment.md, "Static-only apps".
#
#   docker build --file examples/kanban/Dockerfile --tag kanban:local .
#
# Run from the repository root: the build context is the whole repository,
# because the toolchain is built from source (Dockerfile.dockerignore keeps
# the context small). docker-compose.yml does this for you.

# The toolchain to build with. By default it is compiled from this
# repository's source, so nothing needs an account or a published image;
# with a published toolchain image, pass
# --build-arg K_TOOLCHAIN=ghcr.io/k-lang/k:0.1.0 and the source stage is
# skipped.
ARG K_TOOLCHAIN=k-toolchain-from-source
ARG RUST_VERSION=1.99

# ---- Toolchain from source -------------------------------------------------
# The `k` binary: compiler, formatter and test runner. No Node or npm is
# involved. Thin LTO and one codegen unit (the release profile) make the
# toolchain a little faster but take much longer to build, so this stage
# turns them off.
FROM rust:${RUST_VERSION}-bookworm AS k-toolchain-from-source
WORKDIR /k
COPY Cargo.toml Cargo.lock rustfmt.toml clippy.toml ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/k/target \
    CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    cargo build --release --locked --package k_cli \
    && cp target/release/k /usr/local/bin/k

FROM ${K_TOOLCHAIN} AS toolchain

# ---- Build -----------------------------------------------------------------
FROM toolchain AS build

WORKDIR /src

COPY examples/kanban .

# `k check` fails the build on a type error, unformatted code or a breaking
# change against storage.lock; `k test` on a failing test. The tests run in
# the fake DOM, so no browser is needed.
RUN k check
RUN k test

# `k build` writes dist/public: index.html and the hashed assets.
RUN k build

# ---- Serve -----------------------------------------------------------------
# The unprivileged nginx image listens on 8080 and runs as a non-root user.
FROM nginxinc/nginx-unprivileged:1.27-alpine

COPY examples/kanban/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /src/dist/public /usr/share/nginx/html

EXPOSE 8080

# busybox wget is in the image; the document answers 200 for every URL.
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1
