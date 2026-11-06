# Deployment

A K app deploys as **one file**. `k build` produces `dist/app`, a self-contained server executable that
validates its configuration, migrates the database, serves HTTP and runs the job worker. It has built-in
commands for operations, so container images need no shell.

## Building

```bash
k check
k build
```

| Output | Contents |
|---|---|
| `dist/app` | the server executable: the server bundle, the JavaScript runtime, every hashed client asset and every migration |
| `dist/public/` | the same hashed client assets, for teams that serve them from a CDN |

- **Copying `dist/app` is a complete deployment:** code, assets and schema can't be out of step. Assets
  are served with long-lived cache headers.
- **`k build` always builds the `.production` environment.** Environment differences are compile-time
  `use` rules plus runtime configuration; there is no `K_ENV` to set. The same executable runs in staging
  and in production, and they differ only in configuration.
- **Server code runs on a JavaScript runtime embedded in the binary** (V8). No Node.js, Bun or npm is
  involved, at build time or at runtime, and the runtime exposes only web-standard APIs such as `fetch`,
  streams and Web Crypto. Development and production use the same runtime.
- The executable links glibc, so the smallest supported base image is `gcr.io/distroless/cc` or an
  equivalent. `scratch` and musl-based images aren't supported.

An edge target, `k build --target edge` for Cloudflare Workers and similar platforms, is deferred by the
design. The compiler would reject edge-incompatible features, such as long-running jobs or direct Postgres
sockets, at compile time.

### Static-only apps

When an app renders on the client and has no server-only code (see
[The app declaration](app.md#rendering-on-the-client)), **`k build` emits only `dist/public`**, which any
static host can serve. There is no `dist/app`, so no migrations or job worker, and no server
configuration is needed. Adding server-only code later brings `dist/app` back.

## Commands

| Command | Does |
|---|---|
| `dist/app serve` (the default) | validates configuration, applies pending migrations, then serves HTTP on `PORT` and runs the background-job worker |
| `dist/app migrate` | validates configuration and applies pending migrations, then exits |
| `dist/app config` | prints every configuration variable the build reads, never their secret values |
| `dist/app probe <url>` | sends `GET <url>`, and exits `0` on a 2xx answer within 3 seconds, `1` otherwise |
| `dist/app jobs list --dead` | lists dead background jobs with their last error |
| `dist/app jobs retry <id>` | puts a dead job back on the queue |

- **`probe` exists for container health checks on images without `curl`:**
  `HEALTHCHECK CMD ["/app", "probe", "http://127.0.0.1:8080/health"]`. The URL is normally a
  [health-check endpoint page](endpoints.md#health-checks) with a named URL, so it can't drift.
- **`migrate`** is for teams that gate schema changes separately from rollout; `serve` then finds nothing
  pending.
- **On Postgres, `serve` and `migrate` hold an advisory lock while migrating,** so several replicas
  starting together apply each migration once. A failed migration exits with status 1, before the port
  opens.

## Startup and shutdown

- **Startup order:** configuration (exit `78` on any problem, see
  [Configuration and secrets](config.md#validated-at-startup-all-at-once)), database connection,
  migrations, then the HTTP listener. A health check therefore fails until the app can actually serve.
- **On `SIGTERM`** the server stops accepting connections, finishes in-flight requests, stops claiming
  jobs and waits for running ones, for up to `K_SHUTDOWN_GRACE` (25 seconds by default, under Docker's and
  Kubernetes' 30-second kill timeout). Then it exits `0`.
- Logs go to standard output as JSON lines.
- **The runtime reads no environment variable that isn't a `config` field,** so `dist/app config` is the
  complete list of what a deployment can set.

## Public origin

**`K_PUBLIC_ORIGIN`**, for example `https://shop.example`, is the origin users and other systems reach the
app at. It is required in production. It is:

- the base of every absolute URL built by [`PublicUrls`](routing.md#absolute-urls-publicurls), and of
  `canonical` links;
- the `Origin` the [request-forgery check](auth.md#request-forgery) expects;
- the scheme that decides whether the session cookie is `Secure` and `__Host-` prefixed.

## Behind a proxy

- **Trusted proxies are configured, never assumed.** `K_TRUSTED_PROXIES` lists the proxies' CIDR ranges.
  Only then is `X-Forwarded-For` used to find the client's address; with the list empty, the socket
  address is used. `serve` logs a warning when requests arrive with `X-Forwarded-For` while the list is
  empty, because every user would otherwise share one address and one rate limit.
- **Serve HTTP/2 at the proxy.** Each open [server stream](streaming.md) holds a request, and browsers
  allow only six HTTP/1.1 connections per origin.

## Several replicas

Everything that must hold across replicas lives in the database, so replicas need no coordination
service:

- sessions and rate-limit counters (`k_session`, `k_rate_limit`);
- the job queue (`k_job`), claimed with `SKIP LOCKED`;
- broadcasts between server streams, over Postgres `LISTEN`/`NOTIFY`.

**Rolling deploys run the previous release against the new schema,** so migrations follow
expand/contract: a column is retired in one release and dropped in a later one
([Persistence](persistence.md#renames-and-rolling-deploys)).

## Containers

The K project publishes `ghcr.io/k-lang/k:<version>`, a minimal image containing the static `k` binary,
for multi-stage builds:

```text
FROM ghcr.io/k-lang/k:0.1.0 AS build
WORKDIR /src
COPY . .
# k check fails on any type error, failing test, or lockfile drift (schema.lock included).
RUN k check
RUN k build

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/dist/app /app
ENTRYPOINT ["/app"]
CMD ["serve"]
HEALTHCHECK CMD ["/app", "probe", "http://127.0.0.1:8080/health"]
```

Secrets reach the container as files, through `NAME_FILE` variables:

```text
services:
  shop:
    image: shop
    environment:
      K_PUBLIC_ORIGIN: https://shop.example
      DATABASE_URL: postgres://shop@db:5432/shop
      DATABASE_PASSWORD_FILE: /run/secrets/db_password
    secrets: [db_password]
```

## Static hosting

An app with `@Rendering(.client)` and no server-only code builds to static files only, in `dist/public`.
Upload them to any static host; there is no server to run.
