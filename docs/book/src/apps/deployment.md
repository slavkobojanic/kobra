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
- **The client is the right-most `X-Forwarded-For` hop that isn't a trusted proxy,** and only when the
  request comes from a trusted proxy, so a client can't pick its own address. List every proxy in the
  chain, and only those: a range that also holds other machines lets them claim any address. An entry
  that isn't an address or a CIDR range stops `serve` with exit status 78. `Forwarded` and
  `X-Forwarded-Proto` aren't read; whether the session cookie is `Secure` follows `K_PUBLIC_ORIGIN`.
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

> **Planned.** The K project will publish `ghcr.io/k-lang/k:<version>`, a minimal image containing the
> static `k` binary. It isn't published yet; the examples build the toolchain in a first stage instead,
> as described below the sample.

With the toolchain image, a multi-stage build looks like this:

```text
FROM ghcr.io/k-lang/k:0.1.0 AS build
WORKDIR /src
COPY . .
# k check fails on any type error, lockfile drift (schema.lock included) or failing test.
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

Until that image is published, `examples/shop/Dockerfile` shows the alternative: a first stage compiles
`k` from the repository with the official `rust` image, and `ARG K_TOOLCHAIN` switches to the published
image when there is one. Nothing in the build needs an account.

## A system of services

`examples/shop` deploys a [workspace](workspaces.md) of two apps, the shop and paybox, its payment
service, with no account anywhere. It is the worked example for everything on this page.

**One Dockerfile, one image per app.** The build stage runs `k check` (which runs the tests) and `k build` at the
workspace root, which builds every app into `dist/<name>/app`. The runtime stage copies one of them,
chosen by a build argument:

```text
FROM toolchain AS build
WORKDIR /src
COPY examples/shop .
RUN k check
RUN k build --out /dist           # /dist/paybox/app, /dist/shop/app

FROM gcr.io/distroless/cc-debian12:nonroot
ARG APP
COPY --from=build /dist/${APP}/app /app
ENTRYPOINT ["/app"]
CMD ["serve"]
```

**Docker Compose** (`examples/shop/docker-compose.yml`) runs Postgres with a database per service,
Mailpit as the SMTP server (with a web inbox), paybox, two shop replicas and Caddy in front of them:

```text
  shop:
    build: { context: ../.., dockerfile: examples/shop/Dockerfile, args: { APP: shop } }
    deploy: { replicas: 2 }
    environment:
      DATABASE_URL: postgres://shop@db:5432/shop
      DATABASE_PASSWORD_FILE: /run/secrets/db_password
      K_SERVICE_TOKEN_FILE: /run/secrets/shop_service_token      # verifies paybox's callbacks
      PAYBOX_URL: http://paybox:8080
      PAYBOX_TOKEN_FILE: /run/secrets/paybox_service_token       # signs the calls to paybox
      PAYBOX_CALLBACK_URL: http://shop:8080                      # any replica
    healthcheck:
      test: ["CMD", "/app", "probe", "http://127.0.0.1:8080/health"]
```

- **Each service has its own secret**, and a caller signs with the secret of the service it calls:
  paybox's `K_SERVICE_TOKEN` is the shop's `PAYBOX_TOKEN`, and the shop's is paybox's
  `PAYBOX_CALLBACK_TOKEN`.
- **Replicas share nothing but the database,** so the proxy needs no sticky sessions, and paybox can call
  back whichever replica the service name resolves to.
- **Secrets are files.** `scripts/init-secrets.sh` writes them into the git-ignored `secrets/`, and only
  `*.example` placeholders are committed.

**Kubernetes** (`examples/shop/deploy/kubernetes`, for a local kind cluster) runs the same images:
Postgres as a StatefulSet, a `Job` per app running `dist/app migrate`, Deployments whose startup,
readiness and liveness probes request `/health`, Services, an Ingress, and ConfigMaps for what isn't
secret. A script creates the Secrets from the same files, and each pod mounts its Secret at
`/run/secrets`, so the `NAME_FILE` variables are the ones Compose uses.

### Kubernetes on kind

`examples/shop/deploy/kubernetes/README.md` has the full walkthrough. In short, at `examples/shop`:

```bash
scripts/init-secrets.sh
docker compose build paybox shop                     # k-paybox:local and k-shop:local
kind create cluster --name k-shop --config deploy/kubernetes/kind-config.yaml
kind load docker-image k-paybox:local k-shop:local --name k-shop
deploy/kubernetes/create-secrets.sh
kubectl apply --kustomize deploy/kubernetes
kubectl --namespace k-shop wait --for=condition=complete job --all --timeout=300s
kubectl --namespace k-shop rollout status deployment/paybox deployment/shop --timeout=300s
kubectl --namespace k-shop port-forward service/shop 8080:8080
```

kind runs its own container runtime, so the local images are loaded rather than pulled; the manifests
use `imagePullPolicy: IfNotPresent`. The operational commands are the built server's own, run in a pod:

| Task | Command |
|---|---|
| What a build reads | `kubectl --namespace k-shop exec deployment/shop -- /app config` |
| Jobs that couldn't be delivered | `kubectl --namespace k-shop exec deployment/paybox -- /app jobs list --dead` |
| Retry one | `kubectl --namespace k-shop exec deployment/paybox -- /app jobs retry <id>` |
| Remove everything | `kind delete cluster --name k-shop` |

Each pod drains for up to `K_SHUTDOWN_GRACE` (25 s) after `SIGTERM`, within its 30-second termination
grace period, so a rolling restart drops no request.

**The deployment is checked against the build.** `dist/app config` is the complete list of what an app
reads, so a test can hold the deployment files to it: the K repository's `crates/k_cli/tests/deploy.rs`
builds the workspace and fails when a container misses a required variable, sets a variable the app
doesn't read (a typo), passes a secret as a plain variable, or pairs the services' tokens wrongly.

## Static hosting

An app with `@Rendering(.client)` and no server-only code builds to static files only, in `dist/public`.
Upload them to any static host; there is no server to run.

`dist/public/index.html` is the app's document: its metadata, its stylesheet and its entry script around
an empty `#k-app`. The host should:

- answer `index.html` for every URL that isn't a file, so a deep link such as `/boards/<id>` reaches the
  client router (the single-page-app fallback);
- send `index.html` with `Cache-Control: no-cache`, so a new deploy is seen on the next load;
- send everything below `/assets/` with `Cache-Control: public, max-age=31536000, immutable`, because
  those names change whenever their content does, and answer 404 for a missing one.

The Kanban example's image does exactly that with nginx:

```text
FROM ghcr.io/k-lang/k:0.1.0 AS build
WORKDIR /src
COPY . .
RUN k check
RUN k build

FROM nginxinc/nginx-unprivileged:1.27-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /src/dist/public /usr/share/nginx/html
EXPOSE 8080
```

```nginx
location /assets/ {
    try_files $uri =404;
    add_header Cache-Control "public, max-age=31536000, immutable" always;
}

location / {
    try_files $uri /index.html;
    add_header Cache-Control "no-cache" always;
}
```

Until the toolchain image is published, `examples/kanban/Dockerfile` builds `k` from this repository in
a first stage, as the shop's does (its build context is the repository root;
`--build-arg K_TOOLCHAIN=ghcr.io/k-lang/k:0.1.0` switches to the published image). Its `k check` runs
the tests too.
