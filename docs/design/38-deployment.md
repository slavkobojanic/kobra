# Deployment

## Decision

`k build` produces **`dist/app`**, one executable that embeds everything the server needs, or, for an app with no server-only code, **static files only**. The executable validates its configuration, migrates, serves HTTP and runs the job worker, and it has built-in commands for operations so container images need no shell.

## What `k build` produces

- **`dist/app` embeds the server bundle, the JavaScript runtime, the hashed client assets** (served with long-lived cache headers) **and the migrations.** Copying that one file is a complete deployment: code, assets and schema can't be out of step.
- The same client assets are also written to `dist/public/`, for teams that serve them from a CDN.
- `k build` always builds the `.production` environment ([23-app-structure.md](23-app-structure.md#environments)). Environment differences are compile-time `use` rules plus runtime configuration ([25-config-and-secrets.md](25-config-and-secrets.md)); there is no `K_ENV` to set.
- The executable links glibc (V8 needs it). The smallest supported base image is therefore `gcr.io/distroless/cc` or an equivalent; `scratch` and musl-based images are not supported.

### Static-only apps

When an app renders on the client and has no server-only code ([23-app-structure.md](23-app-structure.md#rendering-mode)), **`k build` emits only `dist/public`**, which any static host can serve. There is no `dist/app`, so no `serve`, `probe`, migrations or job worker, and no `K_PUBLIC_ORIGIN` or other server configuration is needed. Adding server-only code later brings `dist/app` back.

## Commands

| Command | Does |
|---|---|
| `dist/app serve` (default) | validates configuration, applies pending migrations, then serves HTTP on `PORT` and runs the background-job worker |
| `dist/app migrate` | validates configuration and applies pending migrations, then exits. For teams that gate schema changes separately from rollout; `serve` then finds nothing pending. |
| `dist/app config` | prints every configuration variable the build reads, never their secret values |
| `dist/app probe <url>` | sends `GET <url>`, exits `0` on a 2xx answer within 3 seconds and `1` otherwise |
| `dist/app jobs list --dead` | lists dead background jobs with their last error ([30-background-jobs.md](30-background-jobs.md#retries-and-dead-jobs)) |
| `dist/app jobs retry <id>` | puts a dead job back on the queue |

- **`probe` exists for container health checks on images without `curl`:** `HEALTHCHECK CMD ["/app", "probe", "http://127.0.0.1:8080/health"]`. The URL is normally a health-check endpoint page with a named URL ([27-endpoint-pages.md](27-endpoint-pages.md#health-checks)), so it can't drift.
- **Migrations:** on Postgres, `serve` and `migrate` hold an advisory lock while migrating, so several replicas starting together apply each migration once and the others wait. A failed migration exits with status 1, before the port opens.

## Startup and shutdown

- **Startup order:** configuration (exit 78 on any problem), database connection, migrations, then the HTTP listener. A health check therefore fails until the app can actually serve.
- **On `SIGTERM`** the server stops accepting connections, finishes in-flight requests, stops claiming jobs and waits for running ones, for up to `ServerConfig.shutdownGrace` (`K_SHUTDOWN_GRACE`, default 25 seconds, under Docker's and Kubernetes' default 30-second kill timeout). Then it exits `0`.
- Logs go to stdout as JSON lines from `k.observe`.
- **The runtime reads no environment variable that isn't a `config` field** ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)), so `dist/app config` is the complete list of what a deployment can set.

## Public origin

- **`K_PUBLIC_ORIGIN`** (`ServerConfig.publicOrigin`, for example `https://shop.example`) is the origin users and other systems reach the app at. It is required in production.
- It is the base of every absolute URL (`PublicUrls`, [12-routing.md](12-routing.md#absolute-urls-publicurls)) and of `canonical` links, the `Origin` that the request-forgery check expects, and the scheme that decides whether the session cookie is `Secure` and `__Host-` prefixed ([26-auth-and-sessions.md](26-auth-and-sessions.md#cookies-and-caching)).

## Behind a proxy

- **Trusted proxies are configured, never assumed.** `K_TRUSTED_PROXIES` lists the proxies' CIDR ranges. Only then is `X-Forwarded-For` used to find the client's address; with the list empty, the socket address is used ([27-endpoint-pages.md](27-endpoint-pages.md#client-address-and-proxies)). `serve` logs a warning when requests arrive with `X-Forwarded-For` while the list is empty.
- **Serve HTTP/2 at the proxy.** Each open server stream holds a request, and browsers allow only six HTTP/1.1 connections per origin ([29-server-streaming.md](29-server-streaming.md)).

## Several replicas

Everything that must hold across replicas lives in the database, so replicas need no coordination service:

- sessions (`k_session`) and rate-limit counters (`k_rate_limit`, [26-auth-and-sessions.md](26-auth-and-sessions.md#guards));
- the job queue (`k_job`), claimed with `SKIP LOCKED` ([30-background-jobs.md](30-background-jobs.md#the-queue-and-the-worker));
- broadcasts between server streams, over Postgres `LISTEN`/`NOTIFY` ([29-server-streaming.md](29-server-streaming.md#sharing-events-between-requests)).

**Rolling deploys run the previous release against the new schema,** so migrations follow expand/contract: a column is retired in one release and dropped in a later one ([19-persistence.md](19-persistence.md#rolling-deploys-expand-then-contract)).

## The toolchain image

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

- One executable with embedded assets and migrations makes deployment atomic.
- A built-in `probe` avoids adding a shell and `curl` to the image only for health checks.
- Building only `.production` keeps "the same artifact everywhere" true: staging and production differ only in configuration.
- Keeping shared state in the database keeps deployment to one executable plus one database, however many replicas run.

## Open questions

- Read-only filesystem support needs a writable location for SQLite in single-node deployments.
