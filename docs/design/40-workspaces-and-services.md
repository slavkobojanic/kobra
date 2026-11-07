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
- **`RemoteClient<P>.at(url:token:)`** builds a client for a URL known only at run time, such as a merchant's callback address that is registered with paybox. It goes through the same codecs, timeouts and error mapping.

## Compatibility between deploys

- **A serving app records the shape of each protocol it implements in `rpc.lock`,** in the same way as `storage.lock` for stored data ([30-background-jobs.md](30-background-jobs.md)): the rpcs, the Protobuf field numbers, and the error cases.
- **`k check` rejects a change that would break callers which are already deployed:** removing an rpc, a field or an error case, changing a field's type, or reusing a field number. The change compiles only when it is marked `@Breaking("reason")`. Adding an rpc, an optional field or an error case is always compatible.

## Tooling

- **Commands run at the workspace root apply to every member:** `k check`, `k test`, `k build` (each app to `dist/<name>/`).
- **`k dev` at the root starts every app member,** each on its own port. In `.development`, a `remote` rule resolves to the sibling member's dev server automatically, with a generated development secret, so no `PAYBOX_URL` needs to be set.
- **Tests stay hermetic.** In `.test`, a `remote` rule binds an in-process client to the serving member's implementation, when it is in the workspace, or to a `TestRemote<P>` double. No sockets are opened.
- **The language server indexes the whole workspace,** so go to implementation on `Payments.charge` jumps from shop's call into paybox's implementation.

## Why

- Shared contract libraries keep both sides compile-checked against the same types. Keeping the dependency graph acyclic keeps builds incremental and parallel, as Bazel requires.
- Running a protocol over the existing RPC stack means services get typed errors, Protobuf and streaming without a second framework.
- `rpc.lock` catches the classic microservice failure: deploying a server that no longer understands the clients already deployed.

## Open questions

- Service discovery beyond a configured URL (DNS SRV, a registry) is left to the platform.
- Whether `k build` at the root should also emit a compose file, or leave that to hand-written examples (examples/stack).
- Streaming rpcs between services can be planned with the browser streaming client ([29-server-streaming.md](29-server-streaming.md)).
