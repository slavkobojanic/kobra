# Workspaces, services and deployment

Sources: `book/apps/workspaces.md` and `deployment.md`, `examples/stack` and `examples/shop`, and
`design/38-deployment.md` and `40-workspaces-and-services.md`.

## Workspaces

A workspace is a root `k.toml` with members, each an app (it has an `App.k`) or a library (it
doesn't):

```toml
# stack/k.toml
[workspace]
members = ["pay-contracts", "paybox", "shop"]
k = "0.1"
```

```toml
# stack/shop/k.toml
[package]
name = "shop"
k = "0.1"

[dependencies]
pay-contracts = { workspace = true }
```

- Dependencies form a DAG (a cycle is K8104). A library keeps its modules' own paths
  (`import pay.Payments`).
- At the root, `k check`, `k test`, `k fmt`, `k build` (into `dist/<name>/`) and `k dev` (every app,
  on ports 8080, 8081, … in dependency order, each wired to the others) work on every member.

## Calling another service

Shared protocols live in a library. A service-to-service protocol is guarded by
`ServiceAuthenticated` (`k.rpc`), and the server learns who called with `@Inject let caller:
ServiceCaller` (`caller.name`).

The caller describes where the server is with a `config` that implements `RemoteService`, and binds the
protocol in `App.k`:

```text
public config PayboxService implements RemoteService {
    @Env("PAYBOX_URL") let url: Url
    @Env("PAYBOX_TOKEN") let token: Secret<String>
    @Env("PAYBOX_TIMEOUT") let timeout: Duration = 10.seconds
}

public app Shop {
    use remote(PayboxService) for Payments
}
```

The serving app verifies the token with its own `K_SERVICE_TOKEN`, which holds the same secret. When an
address is known only at run time, use `RemoteClient.at<P>(url:token:timeout:)`. In tests, a
`service FakePayments implements Payments` in the test file and `use FakePayments for Payments` replace the
remote. The serving app's `rpc.lock` protects deployed callers from breaking changes. Read
`examples/stack` (`read_example`) before you build one. The declarations above are abridged from it
(doc comments removed), and it passes `k check`.

## Building and running

- `k check` then `k build` gives `dist/app`, a single self-contained executable holding the server,
  the runtime, the hashed client assets and the migrations, and `dist/public/`, the same assets for a
  CDN. A client-rendered app with no server-only code builds to `dist/public/` only.
- `dist/app serve` (the default) validates config, applies migrations, then serves HTTP on `PORT` and
  runs the job worker. `dist/app migrate` applies migrations only. `dist/app config` prints the
  variables. `dist/app probe http://127.0.0.1:8080/health` is for container health checks.
- Set `K_PUBLIC_ORIGIN` in every deployment, and `K_TRUSTED_PROXIES` behind a proxy. Proxy WebSocket
  upgrades for `/_k/channels`.
- There's no `K_ENV`. The same build runs in staging and production, and they differ only in
  configuration.
- Several replicas need Postgres. Broadcasts, channels and rate limits go through the database.

```text
FROM ghcr.io/slavkobojanic/kobra:0.1.0 AS build
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

(A published toolchain image is **Planned**. The examples build their own `toolchain` image from
`docker/toolchain.Dockerfile`.) Static apps are served by any web server from `dist/public`
(`examples/kanban/nginx.conf`). `read_doc book/apps/deployment.md` covers Docker Compose, Kubernetes
on kind, and the proxy settings.
