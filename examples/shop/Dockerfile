# syntax=docker/dockerfile:1
#
# One Dockerfile for both apps of the workspace. The Kobra toolchain checks and
# builds every member, and only the chosen app's single executable,
# dist/<app>/app, is copied into a minimal runtime image
# (docs/design/38-deployment.md):
#
#   docker build --file examples/shop/Dockerfile --build-arg APP=paybox --tag k-paybox:local .
#   docker build --file examples/shop/Dockerfile --build-arg APP=shop   --tag k-shop:local .
#
# Run from the repository root: the build context is the whole repository,
# because the toolchain is built from source (Dockerfile.dockerignore keeps
# the context small). docker-compose.yml does all of this for you.

# The toolchain to build with. By default it is compiled from this
# repository's source, so nothing needs an account or a published image;
# with a published toolchain image, pass
# --build-arg K_TOOLCHAIN=ghcr.io/slavkobojanic/kobra:0.1.0-rc.2 and the source stage is
# skipped.
ARG K_TOOLCHAIN=k-toolchain-from-source
ARG RUST_VERSION=1.99

# ---- Toolchain from source -------------------------------------------------
# The `k` binary: compiler, formatter, test runner and the server runtime
# that dist/app embeds. V8 comes prebuilt with the `v8` crate. Thin LTO and
# one codegen unit (the release profile) make the toolchain a little faster
# but take much longer to build, so this stage turns them off.
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

# ---- Build -------------------------------------------------------------------
FROM toolchain AS build
WORKDIR /src
COPY examples/shop .

# `k check` at the workspace root fails the build on a type error in any
# member, on a breaking change against the committed lockfiles (schema.lock,
# storage.lock and rpc.lock: a contract change that would break the other
# service, already deployed), or on a failing test. It runs every member's
# tests, against in-memory SQLite and without opening a socket (paybox is a
# TestRemote double), so no database or service is needed.
RUN k check

# `k build` at the root compiles every app of the workspace, each into one
# self-contained executable with the JavaScript runtime, its client assets
# and its migrations embedded: dist/paybox/app and dist/shop/app.
RUN k build --out /dist

# ---- Runtime -----------------------------------------------------------------
# distroless/cc provides glibc and libstdc++, which the embedded V8 runtime
# needs, and nothing else: no shell, no package manager, no curl. The image
# runs as an unprivileged user.
FROM gcr.io/distroless/cc-debian12:nonroot

# Which app this image runs: paybox or shop.
ARG APP
COPY --from=build /dist/${APP}/app /app

ENV PORT=8080
EXPOSE 8080

# `nonroot`, by number, so Kubernetes' runAsNonRoot can verify it.
USER 65532:65532

# The image has no curl, so the executable probes itself. /health is a named
# URL in both apps (`page HealthCheck at "/health"`), so it can't drift from
# this line.
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
    CMD ["/app", "probe", "http://127.0.0.1:8080/health"]

# `serve` validates every config the production build needs, applies pending
# migrations, then serves HTTP and runs background jobs. `migrate` (used by
# the Kubernetes Job) only applies the migrations.
ENTRYPOINT ["/app"]
CMD ["serve"]
