# Configuration and deployment

A K app deploys as **one file**. `k build` produces `dist/app`, a self-contained server executable that
embeds the JavaScript runtime, the server bundle, every client asset and every migration. Deploying means
copying that file, or putting it in a minimal container.

## Building

```bash
k check
k build
```

| Output | Contents |
|---|---|
| `dist/app` | the server executable, with everything embedded |
| `dist/public/` | the hashed client assets, for teams that serve them from a CDN |

`k build` always builds the `.production` environment. The same executable runs in staging and in
production; they differ only in configuration.

**Server code runs on a JavaScript runtime embedded in the binary** (V8). No Node.js, Bun or npm is
involved, at build time or at runtime, and the runtime exposes only web-standard APIs such as `fetch`,
streams and Web Crypto. Development and production use the same runtime.

> **Planned.** An edge target, `k build --target edge`, for Cloudflare Workers and similar platforms, is
> deferred. The compiler would reject edge-incompatible features, such as long-running jobs or direct
> Postgres sockets, at compile time.

## Configuration and secrets

> **Proposal.** This section follows `docs/design/proposals/config-and-secrets.md`.

Configuration is declared in K, with a `config` declaration, and read from the process environment at
startup:

```k
// src/payments/StripeWebhookConfig.k

/// Settings for verifying Stripe webhook signatures.
public config StripeWebhookConfig {
    /// The endpoint's signing secret.
    @Env("STRIPE_WEBHOOK_SECRET") public let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from the current time.
    @Env("STRIPE_WEBHOOK_TOLERANCE") public let tolerance: Duration = 5.minutes
}
```

- **Every field names its environment variable with `@Env`.** Names are never derived from field names,
  because a variable name is a contract with something outside the codebase.
- **A field with a default is optional; one without is required.** Values are typed: a port is an `Int`,
  a timeout a `Duration`, an origin a `Url`.
- **A config is injected like a service, but only into server-side code.** Injecting one into a component,
  or importing a config type into client code, is a compile error. Secrets can't reach the browser.
- **`Secret<T>`** interpolates as `Secret(•••)`, can't be serialized or compared, and gives up its value
  only through `.reveal()`.

```k
public service StripeWebhookVerifier {
    @Inject let config: StripeWebhookConfig
}
```

### Where values come from

| Environment | Values come from |
|---|---|
| `.production` (`dist/app`) | the process environment only |
| `.development` (`k dev`) | the process environment, plus an optional, git-ignored `.env` file |
| `.test` (`k test`) | never the environment; each test group binds the configs it needs |

For a secret, a variable named `NAME_FILE` may point at a file holding the value. That is how Docker and
Kubernetes deliver secrets, so they work without entrypoint scripts.

### Validated at startup, all at once

When `dist/app` starts, it builds and validates every config reachable from the app's dependency graph
before it opens a port. Any problem stops it, with every problem listed and exit status `78`:

```text
error: configuration is invalid (2 problems)
  STRIPE_WEBHOOK_SECRET     missing (required by payments.StripeWebhookConfig.signingSecret;
                            set STRIPE_WEBHOOK_SECRET or STRIPE_WEBHOOK_SECRET_FILE)
  STRIPE_WEBHOOK_TOLERANCE  "5 minutes" is not a Duration (expected e.g. 300s, 5m)
```

"Reachable" is computed from the `use` rules in `App.k`: a config injected only by the production payment
provider is required only in production. `dist/app config` prints every variable the build reads, without
secret values.

### Standard variables

| Variable | Purpose | Default |
|---|---|---|
| `PORT` | the HTTP port | `8080` |
| `K_PUBLIC_ORIGIN` | the origin users reach the app at, used for absolute URLs | required in production |
| `DATABASE_URL` | `postgres://…` or `sqlite:path` | |
| `DATABASE_PASSWORD` | the database password, as a secret | optional |
| `K_SHUTDOWN_GRACE` | how long to finish in-flight work on `SIGTERM` | `25s` |
| `K_JOB_CONCURRENCY` | background jobs run at once per process | `4` |

## Running the server

> **Proposal.** The commands and startup behavior of `dist/app` follow
> `docs/design/proposals/deployment.md`.

| Command | Does |
|---|---|
| `dist/app serve` (the default) | validates configuration, applies pending migrations, then serves HTTP on `PORT` and runs the background-job worker |
| `dist/app config` | prints every configuration variable the build reads, never their secret values |
| `dist/app probe <url>` | sends `GET <url>`, and exits `0` on a 2xx answer within 3 seconds, `1` otherwise |

- **Startup order:** configuration, database connection, migrations, then the HTTP listener. A health
  check therefore fails until the app can actually serve.
- **Migrations run at startup.** On Postgres, replicas starting together apply each migration once.
- **On `SIGTERM`**, the server stops accepting connections, finishes in-flight requests and running jobs,
  then exits.
- Logs go to standard output as JSON lines.

## Containers

The executable needs glibc, so the smallest supported base image is a distroless image with a C runtime:

```text
FROM ghcr.io/k-lang/k:0.1.0 AS build
WORKDIR /src
COPY . .
RUN k check
RUN k build

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/dist/app /app
ENTRYPOINT ["/app"]
CMD ["serve"]
HEALTHCHECK CMD ["/app", "probe", "http://127.0.0.1:8080/health"]
```

The health URL is normally an [endpoint page](routing.md#endpoint-pages) with a named URL, so the
container configuration can't drift from the code.

## Static hosting

An app with `@Rendering(.client)` and no server-only code builds to static files only, in `dist/public`.
Upload them to any static host; there is no server to run. See [The app declaration](app.md).
