# syntax=docker/dockerfile:1
#
# Huddle's image: the Kobra toolchain checks and builds the app, and only its
# single executable, dist/app, is copied into a minimal runtime image
# (docs/design/38-deployment.md). docker-compose.yml builds it:
#
#   docker build --file examples/huddle/Dockerfile --tag k-huddle:local .
#
# Run from the repository root: the build context is the whole repository,
# because the toolchain is built from source (Dockerfile.dockerignore keeps
# the context small).

# The toolchain to build with: compiled from this repository's source by
# default, so nothing needs an account or a published image. With a
# published toolchain image, pass
# --build-arg K_TOOLCHAIN=ghcr.io/slavkobojanic/kobra:0.1.0-rc.2 and the source stage is
# skipped.
ARG K_TOOLCHAIN=k-toolchain-from-source
ARG RUST_VERSION=1.99

# ---- Toolchain from source -------------------------------------------------
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
COPY examples/huddle .

# Type errors, a breaking change against rpc.lock or schema.lock (open tabs
# are deployed clients), or a failing test fail the build. The tests run
# in-process, against in-memory SQLite, without a socket.
RUN k check
RUN k build --out /dist

# ---- Runtime -----------------------------------------------------------------
# distroless/cc has the glibc and libstdc++ the embedded V8 needs, nothing
# else, and runs as an unprivileged user.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /dist/app /app

ENV PORT=8080
EXPOSE 8080
USER 65532:65532

# No curl in the image: the executable probes itself. The sign-in page
# answers 200 to anyone.
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
    CMD ["/app", "probe", "http://127.0.0.1:8080/sign-in"]

ENTRYPOINT ["/app"]
CMD ["serve"]
