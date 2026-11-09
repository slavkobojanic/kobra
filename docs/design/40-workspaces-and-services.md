# Workspaces and Services

## Decision

**Several K projects can live in one workspace, and one K app calls another through a shared protocol.** Microservices need no extra tooling:

- **A workspace is a root `k.toml` with a `[workspace]` table** listing its member projects.
- **Contracts live in a library package.** A library holds the protocols and the types they carry. The serving app implements a protocol from it; a calling app injects the same protocol and binds it with a `remote` rule in `App.k`.
- **Dependencies form a DAG.** Cycles are compile errors at every level: between modules ([16-modules.md](16-modules.md#modules)), between packages, and between workspace members. Apps can't be depended on at all, only libraries, so two services that call each other share a contract library instead of depending on each other.

```text
stack/
├── k.toml                 # [workspace] members = ["pay-contracts", "paybox", "shop"]
├── pay-contracts/         # library: protocol Payments, protocol PaymentEvents, their types
├── paybox/                # app, server-only: implements Payments, calls PaymentEvents
└── shop/                  # app: implements PaymentEvents, calls Payments
```

```toml
# stack/k.toml
[workspace]
members = ["pay-contracts", "paybox", "shop"]

# stack/shop/k.toml
[package]
name = "shop"
k = "0.1"

[dependencies]
pay-contracts = { workspace = true }
```

## Calling another service

```k
// pay-contracts/src/pay/Payments.k
public protocol Payments requires ServiceAuthenticated {
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge
    rpc refund(chargeId: ChargeId) throws RefundError -> Refund
}

// shop/src/payments/PayboxService.k: where the other service is
public config PayboxService implements RemoteService {
    @Env("PAYBOX_URL") let url: Url
    @Env("PAYBOX_TOKEN") let token: Secret<String>
    @Env("PAYBOX_TIMEOUT") let timeout: Duration = 10.seconds
}

// shop/App.k
public app Shop {
    use remote(PayboxService) for Payments
}
```

- **A protocol from a dependency that the app doesn't implement must have a `remote` rule.** Without one, injecting it is a compile error, because there is nothing to call. A protocol the app implements itself never gets a `remote` rule.
- **Remote clients are server-only.** A browser never holds a service token, and calls between services never need CORS. When the browser needs data from another service, it calls its own app's server, which calls the other service.
- **The wire format is the app's own RPC wire format** ([28-rpc-wire-format.md](28-rpc-wire-format.md)). Errors arrive typed, and network failures surface as `rpcFailure(.offline)` and `.timeout`, exactly as they do in the browser.
- **Calls are authenticated with a signed service token.** The calling app signs a short-lived token from the shared secret in its `RemoteService` config: the caller's name, an expiry and an HMAC. The stdlib guard `ServiceAuthenticated` verifies it on the serving side, and an injected `ServiceCaller` gives the verified caller's name. Mutual TLS is left to the platform.
- **`RemoteClient.at(url:token:)`** builds a client of protocol `P` for a URL known only at run time, such as a merchant's callback address that is registered with paybox. K has no explicit type arguments in expressions, so **`P` comes from the expected type**, the type the result is used as: `let events: PaymentEvents = RemoteClient.at(url: hook, token: secret)`. It goes through the same codecs, timeouts and error mapping.

## Compatibility between deploys

- **A serving app records the shape of each protocol it implements in `rpc.lock`,** in the same way as `storage.lock` for stored data ([30-background-jobs.md](30-background-jobs.md)): the rpcs, the Protobuf field numbers, and the error cases.
- **`k build` writes `rpc.lock` and `k check` checks the code against it,** like `schema.lock`, which `k migrate` (and `k build`) write and `k check` only compares ([13-tooling.md](13-tooling.md#lockfiles)), so CI catches a contract change nobody built.
- **`k check` rejects a change that would break callers which are already deployed:** removing an rpc, a field or an error case, changing a field's type, or reusing a field number. The change compiles only when it is marked `@Breaking("reason")`. Adding an rpc, an optional field or an error case is always compatible.

## Tooling

- **Commands run at the workspace root apply to every member:** `k check`, `k test`, `k build` (each app to `dist/<name>/`).
- **`k dev` (and `k run`) at the root starts every app member,** each on its own port, with one command. In `.development`, a `remote` rule resolves to the sibling member's dev server automatically, with a generated development secret, so no `PAYBOX_URL` needs to be set. Each app's log lines start with its name, and Ctrl-C stops them all.
- **Tests stay hermetic.** In `.test`, a `remote` rule binds an offline `TestRemote<P>` double by default, whose calls fail with `rpcFailure(.offline)`, so each service's tests run without the others; a test that needs answers binds its own fake with a `use` rule, `use FakePayments for Payments`. No sockets are opened. Binding the serving member's own implementation in-process is planned (see "Decided while implementing").
- **The language server indexes the whole workspace,** so go to implementation on `Payments.charge` jumps from shop's call into paybox's implementation.

## Decided while implementing

**Projects and dependencies**

- **A library's modules keep their own paths.** `pay-contracts/src/pay/Payments.k` is `pay.Payments` in every member that depends on it, imported as `import pay.Payments`; the package name is not a prefix. A module belongs to one package of a build: a member and a library (or two libraries) declaring the same module is `K8106`, which keeps module paths unique. Git packages differ ([18-packages.md](18-packages.md)): their module paths are prefixed by their URL name, and an `[aliases]` entry gives them a short prefix; a workspace library needs neither.
- **A dependency names the library's `[package] name`**: `pay-contracts = { workspace = true }`. That table is the only form a workspace dependency has; a git tag (`"v1.2.0"`) is reported as not resolvable yet (`K8102`).
- **The root is not a package.** A root `k.toml` with `[workspace]` has no `[package]`; members are directories below the root, without `..` or hidden directories, never inside one another, and their package names are unique. A member's `k.toml` can't declare a workspace of its own.
- **Diagnostics:** `K8101` malformed manifests, `K8102` unknown dependency, `K8103` a member without `k.toml`, `K8104` a cycle between members (reported at every edge of the cycle, with the cycle), `K8105` a dependency on an app, `K8106` a module in two packages. A reported edge is left out of the graph, so every member is still checked.
- **What a member sees of a library**: its `public` declarations, from its non-test sources. A library's tests, lockfiles and migrations stay its own; its test-only (`testing`) modules are visible to the member's tests.
- **A library's protocols take their Protobuf package from the library**: `/pay_contracts.pay.Payments/Charge` for both paybox and shop, whatever the app's name.

**Calling another service**

- **`RemoteService` is a marker interface** whose fields the compiler checks: a config implementing it has `url: Url` and `token: Secret<String>`, and may have `timeout: Duration` (10 seconds without one), else `K4025`. A `config` may now have an `implements` clause.
- **The rules of `remote`**: the argument names a `RemoteService` config (`K4025`); the target is a protocol (`K4024`) that the app's own services don't implement (`K4024`; a test's fake doesn't count) and that comes from a library (`K4024` for the app's own protocol, which no other service can implement). An injected library protocol that the app neither implements nor binds remotely is `K4023`, in apps only, test code excepted. `remote` rules take `in .env` like any rule and count for environment coverage (`K4015`).
- **Remote clients are server-only** because a protocol with a `remote` rule is a server-only injection: injecting it from code that reaches the browser is `K4007`, with the path ending "a `remote` client, which holds a service token, is server-only". `RemoteClient` itself is `@ServerOnly`, and its result must be a protocol (`K4026`).
- **The service token** is `v1.<caller>.<expiry>.<mac>`: the calling app's name, an expiry one minute ahead, and the HMAC-SHA256 of both under the secret, sent as `x-k-service-token`. The serving app verifies it with `K_SERVICE_TOKEN`, the stdlib's `ServiceConfig`, which every app serving a `ServiceAuthenticated` protocol requires; it is the same secret as the callers' `token`. A token that is malformed, forged, expired or valid for more than two minutes (a skewed clock) is refused like a missing one (`rpcFailure(.rejected(.unauthenticated))` for the caller). In-process calls of the app's own code skip the check. `ServiceCaller.name` is `String?`: `nil` outside a verified service call.
- **Streaming rpcs between services aren't supported yet**: a remote client of a `Stream` rpc panics with the rpc's path (open question below).

- **A remote client is a protocol-typed value, but a service isn't one.** A test fake can be bound to a protocol with `use FakePaybox for Payments`, yet a service can't be returned where a protocol type is expected (`K2001`). Code that builds clients with `RemoteClient.at` therefore keeps the call behind its own interface (paybox's `MerchantNotifier`, implemented by `RemoteNotifier`), which tests bind to an in-process double.
- **Each serving app has its own secret, and a caller signs with the server's.** In `examples/shop` the shop signs its calls with paybox's `K_SERVICE_TOKEN` (`PAYBOX_TOKEN`), and paybox signs its callbacks with the shop's (`PAYBOX_CALLBACK_TOKEN`, an optional `Secret<String>?` that falls back to paybox's own `K_SERVICE_TOKEN`, which is what `k dev`'s one development secret needs). A config field of type `Secret<T>?` is a secret like `Secret<T>` (`NAME_FILE`, redacted).
- **App code may inject a stdlib config**, such as `k.rpc.ServiceConfig` for that fallback: it is read from the environment like the app's own configs, and its variables join the build's configuration.

**`rpc.lock`**

- **One per serving app**, next to its `k.toml`, recording every protocol its own services implement (the browser-facing ones too, since open tabs are deployed callers): its wire service name (`service`), each rpc's parameters, result and error type, and every struct (fields with Protobuf numbers, plus `reserved` numbers of removed fields) and enum (its cases, each with its payload fields) they carry. The format is in [sema.md §12.16](../compiler/sema.md#1216-lockfiles). It replaces the `protocol.lock` of the earlier design ([13-tooling.md](13-tooling.md#lockfiles)).
- **Breaking** (`K8001` at the protocol, one note per change, unless `@Breaking`): removing an rpc, a parameter, a field (a struct's, or one of a case's payload) or a case; changing the type of any of them; adding a required parameter or field; changing the protocol's wire name (renaming the project or moving the protocol to another module; a lock without `service` isn't compared); no longer serving a recorded protocol (at the app; delete its entry by hand once no caller remains). **Compatible:** a new rpc, an optional field or parameter, a new case. A type shared by two protocols breaks both.
- **`k build` writes the lock** (creating it, recording compatible additions, and recording a change marked `@Breaking`); `k check` only reads it, so CI catches what a developer didn't build. Field numbers are kept while a field exists, a new field takes the next number after every number used or reserved, and a removed field's number is reserved forever, so a number is never reused.

**Tooling**

- **`k check`, `k test` and `k build` at the root** apply to every member, in dependency order; in a member they apply to it alone, with its libraries. `k test` prints `== <name>` before each member's report; `--watch` runs in a member only. `k build` at the root writes each app to `dist/<name>/` below the root (or `--out`), skips libraries, and updates each app's `rpc.lock`.
- **`k dev` at the root** builds and serves every app member on `--port`, `--port + 1`, … in dependency order. Each `remote` rule whose protocol a sibling member serves gets the sibling's address as its config's `url` variable and one development secret, generated per run, as its `token`; every member gets the same secret as `K_SERVICE_TOKEN`. These values win over a member's `.env` (the process environment still wins over both). A change anywhere, a member's `.env` included, rebuilds and restarts every app; a build with errors keeps the last good builds serving. **Every line an app prints starts with its name**, padded to the longest (`shop   | serving on http://localhost:8081`, `paybox | info: …`), so the interleaved logs say whose they are; the server's own log lines are then text rather than JSON. A port that is taken is reported with the app's name and the port.
- **`k run` at the root** does the same without watching for changes: every app, wired the same way, until Ctrl-C or `SIGTERM` stops them all.
- **An app names its own development address with `[dev.env]`** ([24-projects.md](24-projects.md#development-settings)): the shop's `PAYBOX_CALLBACK_URL = "http://127.0.0.1:{port}"` is right whatever `--port` is and whichever app starts first. Only `remote` rules are wired automatically; an address an app hands to another at run time is its own to give.
- **A restart waits at most 2 s** for requests in flight (production waits `K_SHUTDOWN_GRACE`, 25 s), and open streams end at once ([29-server-streaming.md](29-server-streaming.md#the-handler)), so a rebuild serves again within a second or two.
- **In `.test`, every `remote` rule binds `TestRemote<P>`**, even when the serving app is a member of the workspace. Binding the serving member's implementation in-process is deferred: two apps would share one test isolate's process state (configuration, sessions, the test database), which needs per-app runtime state first.
- **`k dev` gives each member its own development database.** A relative `sqlite:` path in a member's `DATABASE_URL` (the development default `sqlite:.k/<package>.db`, or one from `.env`) is relative to that member's directory, like a relative `NAME_FILE` path, and its directory is created, so two apps run at the root never share a file. `k run` and `k migrate` resolve it the same way.
- **The language server** loads every member with its libraries mounted, so definitions go to the library's own files, an edit of a library re-checks every member that uses it, and go to implementation from shop's `payments.charge` reaches paybox's handler. Find references on an interface or protocol method now also lists the methods implementing it and the calls through concrete types, as rename covers them.

## Why

- Shared contract libraries keep both sides compile-checked against the same types. Keeping the dependency graph acyclic keeps builds incremental and parallel, as Bazel requires.
- Running a protocol over the existing RPC stack means services get typed errors, Protobuf and streaming without a second framework.
- `rpc.lock` catches the classic microservice failure: deploying a server that no longer understands the clients already deployed.

## Open questions

- Service discovery beyond a configured URL (DNS SRV, a registry) is left to the platform.
- Whether `k build` at the root should also emit a compose file and Kubernetes manifests, or leave them hand-written: `examples/shop` writes both by hand, and `crates/k_cli/tests/deploy.rs` checks them against each app's `dist/app config`, which a generator would make unnecessary.
- Streaming rpcs between services can be planned with the browser streaming client ([29-server-streaming.md](29-server-streaming.md)).
- The in-process `.test` binding to a serving member's implementation (see above).
- Per-caller secrets (today one secret per serving app, shared by its callers), and rotating them without downtime.
