# Workspaces and services

A system often grows past one app: a payment service, a storefront, an admin tool. Kobra keeps them in one
**workspace**, shares their contracts through **library packages**, and lets one app call another with
the same protocols, codecs and typed errors it already uses between its browser and its server. There is
no second framework to learn.

The smallest complete example is `examples/stack` in the Kobra repository; `examples/shop` is the same
shape grown into a production-style system, with a fake card provider, databases, background jobs,
Docker Compose and Kubernetes (see [Deployment](deployment.md#a-system-of-services)):

```text
stack/
├── k.toml                 # [workspace] members = ["pay-contracts", "paybox", "shop"]
├── pay-contracts/         # library: protocols Payments and PaymentEvents, their types
├── paybox/                # app: serves Payments, calls PaymentEvents
└── shop/                  # app: serves PaymentEvents, calls Payments
```

## A workspace

The root `k.toml` lists the member projects. It is not a package itself, so it has no `[package]`.

```toml
# stack/k.toml
[workspace]
members = ["pay-contracts", "paybox", "shop"]
k = "0.1"
```

A workspace builds with one toolchain: `k` is the requirement for every member, which then needn't repeat
it (a member's own `k` may narrow it but not contradict it, `K8107`), and `k build` records the exact
toolchain in the root's `k.lock` ([Toolchains](../reference/cli.md#toolchains)).

Each member is an ordinary project with its own `k.toml`. A member depends on a library of the same
workspace by the library's `[package] name`:

```toml
# stack/shop/k.toml
[package]
name = "shop"
k = "0.1"

[dependencies]
pay-contracts = { workspace = true }
```

A library is a project without `App.k`. Its **modules keep their own paths**, so shop imports the
library's types exactly like its own:

```k
// examples/stack/shop/src/checkout/Checkout.k
import pay.ChargeRequest
import pay.Payments
```

No prefix and no alias is needed: two packages of one build can't declare the same module (`K8106`), so
`pay.Payments` names exactly one module. (Git packages, when they arrive, are different: their module paths
start with their URL, and an alias in `k.toml` shortens it; see [Packages](packages.md).)

What shop sees of the library is its `public` declarations. The library's tests, lockfiles and migrations
stay its own.

### The dependency graph is a DAG

Dependencies can't form a cycle, at any level: between modules, between packages and between members.
**An app can't be depended on at all.** Two services that call each other share a contract library
instead: paybox calls `PaymentEvents`, which shop serves, and shop calls `Payments`, which paybox serves,
and both protocols live in `pay-contracts`.

| Problem | Code |
|---|---|
| `k.toml` can't be read, or lacks `name` or `k` | `K8101` |
| `{ workspace = true }` names no member | `K8102` |
| a listed member has no `k.toml` | `K8103` |
| members depend on each other in a cycle | `K8104` |
| a dependency names an app | `K8105` |
| two packages of one build declare the same module | `K8106` |

```text
error[K8104]: `a` and `b` depend on each other
 --> a/k.toml:6:1
  |
6 | b = { workspace = true }
  | ^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: the cycle: a → b → a
  = note: dependencies must form a DAG: move what both need into a library both depend on
```

## Contracts in a library

A protocol between services is an ordinary protocol, usually guarded by `ServiceAuthenticated`, which
admits only the other services of the system:

```k
// examples/stack/pay-contracts/src/pay/Payments.k
import k.rpc.ServiceAuthenticated

/// Takes payments for the other services of the system. Served by paybox
/// and called by shop, never by a browser: `ServiceAuthenticated` admits
/// only callers holding a token signed with the shared secret.
public protocol Payments requires ServiceAuthenticated {
    /// Charges `request.cents` to a card. Throws `.declined` for an amount
    /// that isn't positive.
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge
}
```

The serving app implements it like any protocol. `ServiceCaller` names the service that called:

```k
// examples/stack/paybox/src/payments/PaymentsApi.k
~import k.rpc.ServiceCaller
/// Paybox's implementation of `Payments`.
public service PaymentsApi implements Payments {
    @Inject let caller: ServiceCaller
    @Inject let log: ChargeLog
    @Inject let events: PaymentEvents

    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        guard request.cents > 0 else {
            throw ChargeError.declined
        }

        let charge = Charge(
            id: "ch_{request.orderId}",
            orderId: request.orderId,
            cents: request.cents,
            caller: caller.name ?? "unknown"
        )
        log.record(charge: charge)
        // ...
        return charge
    }
}
```

## Calling another service

The calling app says where the other service runs with a config that implements `RemoteService`, and
binds the protocol to it with a `remote` rule in `App.k`:

```k
// examples/stack/shop/src/payments/PayboxService.k
~import k.rpc.RemoteService
/// Where paybox runs, for `Payments`.
public config PayboxService implements RemoteService {
    /// Paybox's base URL.
    @Env("PAYBOX_URL") let url: Url
    /// The secret paybox verifies shop's calls with.
    @Env("PAYBOX_TOKEN") let token: Secret<String>
    /// How long a call may take.
    @Env("PAYBOX_TIMEOUT") let timeout: Duration = 10.seconds
}
```

```k
// examples/stack/shop/App.k
public app Shop {
    use remote(PayboxService) for Payments
}
```

Then any server-side service injects `Payments` and calls it. Errors arrive typed; a network failure is
`rpcFailure(.offline)` or `.timeout`, exactly as in the browser:

```k
// examples/stack/shop/src/checkout/Checkout.k
/// Pays through paybox. Server-only, since it holds a remote client.
public service Checkout {
    @Inject let payments: Payments

    /// Charges `cents` for `orderId`; the charge's id.
    func pay(orderId: String, cents: Int) async throws CheckoutError -> Receipt {
        do {
            let charge = try await payments.charge(request: ChargeRequest(orderId: orderId, cents: cents))
            return Receipt(chargeId: charge.id, cents: charge.cents)
        } catch ChargeError.declined {
            throw CheckoutError.declined
        } catch ChargeError.rpcFailure(_) {
            throw CheckoutError.paymentsUnavailable
        }
    }
}
```

The rules:

- **A library protocol the app doesn't implement needs a `remote` rule** (`K4023`): otherwise there is
  nothing to call. A protocol the app implements never has one, and neither does the app's own protocol
  (`K4024`): no other service could implement it.
- **`remote(…)` takes a config that implements `RemoteService`**, with `url: Url` and
  `token: Secret<String>`, and optionally `timeout: Duration` (`K4025`).
- **Remote clients are server-only.** A browser never holds a service token, and calls between services
  never need CORS. Injecting a remote protocol from code that reaches the browser is `K4007`:

  ```text
  error[K4007]: `checkout.Checkout` is server-only; this file reaches the browser
   = note: checkout.Checkout → pay.Payments (a `remote` client, which holds a service token, is server-only)
  ```

  When the browser needs another service's data, it calls its own app's server, which calls the other
  service. That is what shop's `Cart` does through its own `CheckoutApi`.
- Like any `use` rule, a `remote` rule can be restricted to environments with `in .production`.

### Service tokens

Each call carries a short-lived **service token**: the caller's name and an expiry, signed with HMAC under
the secret in the caller's `token`. The serving app's `ServiceAuthenticated` guard verifies it with its own
`K_SERVICE_TOKEN`, which must be the same secret. A missing, forged or expired token is refused, and the
caller sees `rpcFailure(.rejected(.unauthenticated))`. Mutual TLS, if you want it, is the platform's job.

### Addresses known at run time

`RemoteClient.at` builds a client for a URL known only at run time, such as a callback address a merchant
registered. The protocol is its type argument, `RemoteClient.at<PaymentEvents>(…)`, or the expected
type, the type you use the result as. Anything but a protocol is `K4026`:

```k
// examples/shop/paybox/src/payments/RemoteNotifier.k
~import k.rpc.RemoteClient
let events = RemoteClient.at<PaymentEvents>(url: callbackUrl, token: config.token ?? service.token, timeout: config.timeout)

do {
    try await events.chargeSucceeded(charge: charge)
} catch .rpcFailure(let failure) {
    return failure
}
```

This is from the [shop workspace](../examples/shop.md)'s paybox, which tells merchants about charges at the
callback URL each one registered.

## `rpc.lock`: deploys that don't break callers

Services are deployed one at a time, so a new paybox must still understand the shop that is already
running. Every app that serves protocols records their shape in `rpc.lock`, next to its `k.toml`:

```toml
[protocols."pay.Payments".rpcs.charge]
params = [{ name = "request", type = "pay.ChargeRequest" }]
returns = "pay.Charge"
throws = "pay.ChargeError"

[types."pay.ChargeRequest"]
fields = [
    { name = "orderId", type = "String", number = 1 },
    { name = "cents", type = "Int", number = 2 },
]
```

`k build` writes it and you commit it; `k check` only reads it, comparing the code with it on every run,
CI included, the way `schema.lock` is written by `k migrate` and checked by `k check`.

- **Compatible:** adding an rpc, an optional field or parameter, a field with a default (a struct's, or an
  enum case's: `case outOfStock(available: Int, restockOn: Date? = nil, notify: Bool = false)`), or an
  error case.
- **Breaking:** removing an rpc, a parameter, a field or an error case, changing any of their types, adding
  a required field or parameter (a struct's fields and an enum case's payload fields follow the same
  rules: `case outOfStock(available: Int)` gaining a required `restockOn` breaks), changing the protocol's wire name (recorded as `service`, from the
  project's name, the module and the protocol), or no longer serving a recorded protocol. Each is `K8001`, with one note
  per change:

  ```text
  error[K8001]: `pay.Payments` changed in a way that breaks callers already deployed, without `@Breaking`
   = note: field `caller` of `pay.Charge` was `String` and is now `String?`
  ```

  Mark the protocol `@Breaking("reason")` once no deployed caller relies on the old shape; the next
  `k build` records the new one.

Field numbers are assigned once: a field keeps its number while it exists, and the number of a removed
field is reserved and never used again.

## Tooling

Run at the workspace root, the commands apply to every member; in a member, to that member with the
libraries it depends on.

```bash
k check            # every member; a library's problems are reported once
k test             # every member's tests, each after a `== <name>` line
k build            # each app into dist/<name>/, and each rpc.lock updated
k dev              # every app, each calling the others
k run              # the same, without rebuilding on change
```

**`k dev` at the root** runs every app, on `8080`, `8081`, … in dependency order (`--port` sets the
first). Each `remote` rule whose protocol a sibling serves is wired to it with a development secret
generated for the run, so you set no `PAYBOX_URL` and no tokens. Every line an app prints starts with its
name, and Ctrl-C stops them all:

```text
paybox | serving on http://localhost:8080
shop   | serving on http://localhost:8081
shop   | mail from orders@shop.example to ana@example.com: Your K Shop order
```

A change anywhere, each member's `.env` included, rebuilds and restarts every app. An address an app
hands to another at run time, such as the shop's callback URL for paybox, is the app's own to give: its
`[dev.env]` names it (`PAYBOX_CALLBACK_URL = "http://127.0.0.1:{port}"`), so it is right whatever port
the app gets.

**Tests stay hermetic.** In `.test`, every `remote` rule binds an offline `TestRemote<P>` double by
default, even when the serving app is in the same workspace: its every call fails with
`rpcFailure(.offline)`, so nothing leaves the process and each service's tests run on their own. Bind
your own fake with a `use` rule to test the happy path:

```k
// examples/stack/shop/src/checkout/Checkout.test.k
/// A paybox that answers in-process.
service FakePayments implements Payments {
    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        guard request.cents > 0 else {
            throw ChargeError.declined
        }

        return Charge(id: "ch_{request.orderId}", orderId: request.orderId, cents: request.cents, caller: "test")
    }
}

test Checkout {
    use FakePayments for Payments

    @Inject let checkout: Checkout

    test "a payment answers paybox's charge" {
        let receipt = try await checkout.pay(orderId: "o1", cents: 990)

        expect receipt.chargeId == "ch_o1"
        expect receipt.cents == 990
    }
    // ...
}
```

> **Planned** (in-process-services, service-streaming). Binding the serving member's own implementation in-process in `.test`, and streaming rpcs
> between services.

**The editor** sees the whole workspace: go to definition from shop's `payments.charge(…)` opens the
library's file, go to implementation opens paybox's handler, and editing the contract re-checks every
member that uses it.
