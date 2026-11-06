# syntax=docker/dockerfile:1
#
# Two stages: the K toolchain checks and builds the app, and only the
# resulting single executable, dist/app, is copied into a minimal runtime
# image. See docs/design/38-deployment.md.

ARG K_VERSION=0.1.0

# ---- Build ---------------------------------------------------------------
# The official toolchain image holds the one static `k` binary: compiler,
# formatter and test runner. No Node or npm is involved.
FROM ghcr.io/k-lang/k:${K_VERSION} AS build

WORKDIR /src

COPY . .

# `k check` fails the build on a type error, unformatted code, a failing
# test, or a breaking change against the committed lockfiles, schema.lock
# and storage.lock. Tests run against in-memory SQLite, so no database is
# needed.
RUN k check

# `k build` compiles the server bundle, client assets and migrations into one
# self-contained executable with the JavaScript runtime embedded.
RUN k build

# ---- Runtime -------------------------------------------------------------
# distroless/cc provides glibc and libstdc++, which the embedded V8 runtime
# needs, and nothing else: no shell, no package manager, no curl. The image
# runs as an unprivileged user.
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=build /src/dist/app /app

ENV PORT=8080
EXPOSE 8080

USER nonroot

# The image has no curl, so the executable probes itself. /health is a named
# URL (`page HealthCheck at "/health"`), so it can't drift from this line.
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
    CMD ["/app", "probe", "http://127.0.0.1:8080/health"]

# `serve` validates every config the production build needs, applies pending
# migrations, then serves HTTP and runs background jobs.
ENTRYPOINT ["/app"]
CMD ["serve"]
