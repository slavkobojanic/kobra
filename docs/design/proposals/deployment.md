# Proposal: Deploying `dist/app`

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[13-tooling.md](../13-tooling.md) says `k build` produces `dist/app`, "a single self-contained server executable, plus hashed client assets", and that deploying means "copying one file, or using a minimal container". It doesn't say what the executable does when started, how a container checks its health without a shell, how it learns its public URL, how it shuts down, or what base image it needs.

## Proposal

### What `k build` produces

- **`dist/app` embeds everything the server needs:** the server bundle, the JavaScript runtime, the hashed client assets (served with long-lived cache headers) and the migrations. Copying that one file is a complete deployment.
- The same client assets are also written to `dist/public/`, for teams that serve them from a CDN.
- `k build` always builds the `.production` environment of [app-structure.md](app-structure.md). Environment differences are compile-time `use` rules plus runtime configuration ([config-and-secrets.md](config-and-secrets.md)); there is no `K_ENV` to set.
- The executable links glibc (V8 needs it). The smallest supported base image is therefore `gcr.io/distroless/cc` or an equivalent; `scratch` and musl-based images are not supported.

### Commands

| Command | Does |
|---|---|
| `dist/app serve` (default) | validates configuration, applies pending migrations, then serves HTTP on `PORT` and runs the background-job worker |
| `dist/app config` | prints every configuration variable the build reads, never their secret values |
| `dist/app probe <url>` | sends `GET <url>`, exits `0` on a 2xx answer within 3 seconds and `1` otherwise |

- **`probe` exists for container health checks on images without `curl`:** `HEALTHCHECK CMD ["/app", "probe", "http://127.0.0.1:8080/health"]`. The URL is normally a named endpoint page ([server-endpoints.md](server-endpoints.md)), so it can't drift from the page.
- **Migrations at startup** follow [migrations.md](migrations.md). On Postgres, `serve` holds an advisory lock while migrating, so several replicas starting together apply each migration once and the others wait. A failed migration exits with status 1, before the port opens.

### Startup and shutdown

- Startup order: configuration (exit 78 on any problem), database connection, migrations, then the HTTP listener. A health check therefore fails until the app can actually serve.
- **On `SIGTERM`** the server stops accepting connections, finishes in-flight requests, stops claiming jobs and waits for running ones ([background-jobs.md](background-jobs.md)), for up to `K_SHUTDOWN_GRACE` (default `25s`, under Docker's and Kubernetes' default 30-second kill timeout). Then it exits `0`.
- Logs go to stdout as JSON lines from `k.observe`.

### Public origin and absolute URLs

- `ServerConfig.publicOrigin` (`K_PUBLIC_ORIGIN`, for example `https://shop.example`) is the origin users and other systems reach the app at. It is required in production and defaults to `http://localhost:<port>` in development and tests.
- **Every generated `Page` value has `url: Url`**, the path and query (`/receipt/0b6f…`), and, in server code, **`absoluteUrl: Url`**, prefixed with the public origin. Code that hands URLs to other systems (a payment provider's return URL, a link in an email) uses `absoluteUrl`, so developers still never write URLs ([12-routing.md](../12-routing.md)).
- `canonical(.page)` in `metadata` renders the absolute URL ([22-metadata.md](../22-metadata.md)).
- **External navigation** uses `Router.navigate(url: Url)`, the counterpart of `link(url:)` in [05-views.md](../05-views.md). It leaves the app with a full page load, for example to a hosted payment page.
- **Default URLs of multi-word page names are kebab case:** `page SignIn` is at `/sign-in` ([12-routing.md](../12-routing.md) shows only one-word names).

### The toolchain image

The K project publishes `ghcr.io/k-lang/k:<version>`, a minimal image containing the static `k` binary, for multi-stage builds:

```dockerfile
FROM ghcr.io/k-lang/k:0.1.0 AS build
WORKDIR /src
COPY . .
RUN k check
RUN k build

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/dist/app /app
ENTRYPOINT ["/app"]
CMD ["serve"]
```

## Why

- One executable with embedded assets and migrations makes deployment atomic: code, assets and schema can't be out of step.
- A built-in `probe` avoids adding a shell and `curl` to the image only for health checks.
- `absoluteUrl` extends "developers don't write URLs" to the places where full URLs are unavoidable.
- Building only `.production` keeps "the same artifact everywhere" true: staging and production differ only in configuration.

## Open questions

- Running migrations as a separate deploy step (`dist/app migrate`) for teams that want schema changes gated separately from rollout.
- Read-only filesystem support needs a writable location for SQLite in single-node deployments.
