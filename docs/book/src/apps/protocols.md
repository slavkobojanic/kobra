# Protocols and RPC

K is full-stack, and the browser talks to the server over **RPC, not REST**. A `protocol` declares the
contract in K. It compiles to **Protobuf** and runs over the **Connect protocol**, which works natively in
browsers and is wire-compatible with gRPC.

You never write a URL, a `fetch` call, a JSON parser or an API client. You call a function, and the
compiler checks both sides against the same contract. RPC is the only way an app's own client talks to
its server; the one escape hatch to raw HTTP, for external systems, is the
[endpoint page](endpoints.md).

## Declaring a protocol

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/cart/Context.k
import k.auth.AuthRole
import k.auth.SessionUserId
import k.rpc.Broadcast
import k.rpc.RpcFailure

struct ProductId implements Equatable, Codable {
    let value: Int
}

struct Product implements Codable {
    let id: ProductId
    let name: String
}

struct CartId implements Equatable, Hashable, Codable {
    let value: Int
}

struct CartItem implements Codable {
    let productId: ProductId
}

enum CartEvent implements Codable {
    case added(CartItem)
    case cleared
}

struct AddItemRequest implements Codable {
    let productId: ProductId
}

enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case rpcFailure(RpcFailure)
}

service CartRepository {
    private let changes: Broadcast<CartId, CartEvent> = Broadcast()

    func items(for owner: SessionUserId) async -> List<CartItem> {
        return []
    }

    func product(_ id: ProductId) async -> Product? {
        return nil
    }

    func add(_ product: Product, for owner: SessionUserId) async -> CartItem {
        return CartItem(productId: product.id)
    }

    func events(cart: CartId) -> Stream<CartEvent> {
        return changes.subscribe(key: cart)
    }
}

enum StaffRole implements AuthRole {
    case admin
}

struct SignInRequest implements Codable {
    let email: String
}

enum SignInError implements Error {
    case invalidCredentials
    case rpcFailure(RpcFailure)
}

struct OrderSummary implements Codable {
    let number: Int
}

enum AdminError implements Error {
    case rpcFailure(RpcFailure)
}

struct OrderRequest implements Codable {
    let productId: ProductId
}

struct PlacedOrder implements Codable {
    let number: Int
}

interface CartActions {
    func add(product: Product) async throws CartError
}

page Home at "/" {
    view {
        text { "Home" }
    }
}

page SignIn at "/sign-in" {
    @Param let returnTo: Page = .home

    view {
        text { "Sign in" }
    }
}

page Cart at "/cart" {
    view {
        text { "Cart" }
    }
}
```
-->

```k
~import k.auth.Authenticated
~import k.auth.RateLimit
// CartProtocol.k: the contract
public protocol CartProtocol requires Authenticated {
    rpc list() throws CartError -> List<CartItem>

    rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)

    rpc watch(cartId: CartId) throws CartError -> Stream<CartEvent>
}
```

- **The signature reads in a fixed order:** `rpc name(params) async throws E -> T requires Guards`.
  `requires` comes after the complete signature, as it does in `page` and `protocol` headers, so the
  effects `async throws E -> T` stay uninterrupted.
- Parameters and return types are ordinary `Codable` structs, enums, collections and core types.
- K's `protocol` means a **wire contract** between processes, not Swift's protocols. In-process
  abstractions use [`interface`](../language/declarations.md#interfaces).

## Implementing it on the server

A service that `implements` a protocol is its server implementation:

```k
~import k.auth.Session
~
~import k.auth.Authenticated
~import k.auth.RateLimit
~
~protocol CartProtocol requires Authenticated {
~    rpc list() throws CartError -> List<CartItem>
~
~    rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)
~
~    rpc watch(cartId: CartId) throws CartError -> Stream<CartEvent>
~}
service CartApi implements CartProtocol {
    @Inject let repository: CartRepository
    @Inject let session: Session

    rpc list() async throws CartError -> List<CartItem> {
        return await repository.items(for: session.user.id)
    }

    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        guard let product = await repository.product(request.productId) else {
            throw CartError.productNotFound
        }

        return await repository.add(product, for: session.user.id)
    }

    rpc watch(cartId: CartId) async throws CartError -> Stream<CartEvent> {
        // …
~        return repository.events(cart: cartId)
    }
}
```

- **A service that implements a protocol is server-only.** The compiler keeps it, and everything it
  injects, out of the client bundle.
- The compiler checks that every `rpc` is implemented, with matching types. A handler is written as an
  `rpc` or a `func` of the same name.
- **`rpc` is only for what a protocol serves.** An `rpc` in a struct, a component, or a service whose
  protocols don't declare it could never be called over the wire, so it is a compile error (`K5508`). Its
  quick fix makes it a `func`; if you meant it to be remote, declare it in the protocol.
- An implementation may be `async` even though the contract doesn't say so: whether the server suspends
  is an implementation detail.
- The protocol's `Authenticated` guard has already run inside `CartApi`, so `session.user` is a
  `SessionUser`, not an optional ([Authentication and sessions](auth.md)).

## Calling it from the client

**Only services inject protocols.** Components use them through services:

```k
~import k.auth.Authenticated
~
~protocol CartProtocol requires Authenticated {
~    rpc add(request: AddItemRequest) throws CartError -> CartItem
~}
service CartService {
    @Inject let api: CartProtocol

    @State var items: List<CartItem> = []

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}
```

**Injection works the same wherever the code runs.** `@Inject let api: CartProtocol` gets a generated
network client in the browser, and a **direct in-process call** during server rendering, with no HTTP
round trip. Guards and validation still run on in-process calls.

## Typed errors cross the wire

`throws CartError` maps to Connect error details, so the client receives the exact error case the server
threw, and client-side `catch` stays exhaustive over server errors.

```k
~import k.ui.Router
~
~component AddButton {
~    @Prop let cart: CartActions
~    @Prop let product: Product
~    @State var message: String? = nil
~
~    func add() async {
do {
    try await cart.add(product: product)
} catch .outOfStock(let available) {
    message = "Only {available} left"
} catch .productNotFound {
    message = "This product is no longer available"
} catch .rpcFailure(.rejected(.unauthenticated)) {
    Router.navigate(to: .signIn.returnTo(.cart))
} catch .rpcFailure {
    message = "Please try again"
}
~    }
~
~    view {
~        text { message ?? "" }
~    }
~}
```

## Guards and middleware: `requires`

`requires` declares guards on a whole protocol or on individual rpcs, such as `Authenticated`,
`Role(.admin)` or `RateLimit(10.perMinute)`. Guards are part of the contract, so clients can see them too.
Cross-cutting interceptors that never reject a call, such as logging and tracing, are configured once in
`App.k`.

**Guards go on the protocol, never on the service that implements it.** The server runs the protocol's
guards before the handler, so a `requires` clause on a handler would never be checked. It is a compile
error (`K5507`) instead, and its quick fix moves the clause to the protocol's rpc, removing the imports
that only the clause used, so no unused import is left behind. If the protocol comes
from a library, its guards are the library's to declare: the handler checks the caller itself, for
example through `session.user`. An `rpc` that no protocol declares is never served, so a clause on it is
the same error.

```k
~import k.auth.Authenticated
~import k.auth.RateLimit
~import k.auth.Role
public protocol AdminProtocol {
    rpc signIn(request: SignInRequest) throws SignInError requires RateLimit(5.perMinute)

    rpc recentOrders() throws AdminError -> List<OrderSummary> requires Authenticated, Role(.admin)
}
```

## Failures outside the handler

An rpc call can fail outside its handler: before it runs (a guard refuses it, or a `Validatable`
parameter fails validation), or on the way between the browser and the server. Those failures are one
standard enum in `k.rpc`:

```k
// crates/k_sema/stdlib/k/rpc/RpcFailure.k
~import k.auth.GuardRejection
~import k.validation.ValidationError
public enum RpcFailure implements Error, Codable {
    /// A guard refused the call; the payload says why.
    case rejected(GuardRejection)
    /// A [`k.validation.Validatable`] parameter failed validation.
    case invalid(ValidationError)
    /// The browser couldn't reach the server.
    case offline
    /// The server didn't answer in time.
    case timeout
}
```

**Every rpc declares an error enum with exactly one case `rpcFailure(RpcFailure)`,** because every rpc
can be called over the network, and the network can fail. The name is fixed, so the runtime never has
to find the case by its shape:

```k
~import k.rpc.RpcFailure
~
public enum CheckoutError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case paymentUnavailable
    case rpcFailure(RpcFailure)
}
```

```k
~import k.auth.RateLimit
~import k.rpc.RpcFailure
~import k.rpc.RpcOnlyError
~
~enum CheckoutError implements Error {
~    case productNotFound
~    case rpcFailure(RpcFailure)
~}
~
public protocol StoreProtocol {
    rpc placeOrder(request: OrderRequest) throws CheckoutError -> PlacedOrder requires RateLimit(10.perMinute)

    rpc featured() throws RpcOnlyError -> List<Product>
}
```

- An rpc whose error enum lacks that case, or that declares no `throws` clause, is a compile error, with
  a quick fix that adds the case.
- **An rpc with no errors of its own declares `throws RpcOnlyError`,** a standard enum whose only case is
  `rpcFailure(RpcFailure)`.
- **Only the runtime produces `rpcFailure`.** A handler can't throw it. Client code that wraps errors at
  a layer boundary passes it on like any other case:

  ```k
  ~import k.rpc.RpcFailure
  ~
  ~enum LookupError implements Error {
  ~    case notFound
  ~    case rpcFailure(RpcFailure)
  ~}
  ~
  ~func lookup(cart: CartActions, product: Product) async throws LookupError {
  ~    do {
  ~        try await cart.add(product: product)
  ~    } catch .productNotFound, .outOfStock {
  ~        throw LookupError.notFound
  ~    }
  catch .rpcFailure(let failure) {
      throw LookupError.rpcFailure(failure)
  }
  ~}
  ```

- `GuardRejection` is `.unauthenticated`, `.forbidden` or `.rateLimited(retryAfter: Duration)`.
- A `@Form` shows `.invalid` under its fields when the submit action rethrows the `ValidationError`
  ([Forms and validation](forms.md)).
- Guards and validation also run on in-process calls during server rendering, so `rejected` and
  `invalid` can happen there too.

### Network failures

- **`offline`:** the browser couldn't send the request or lost the connection before a response arrived
  (no network, a failed DNS lookup, a refused or dropped connection).
- **`timeout`:** no response arrived within the client's rpc timeout.
- **They happen only in the browser.** Calls during server rendering are in-process, and so is every
  call in `k test` ([Testing](testing.md#which-side-a-test-runs-on)). With no network in between,
  `offline` and `timeout` never occur there.
- **A network failure is a value the page handles,** like any other error. A dropped connection while
  submitting a form or saving a draft doesn't lose the user's input through a panic. External APIs work
  the same way: `HttpClient` throws `HttpError.offline` and `.timeout` ([HTTP client](http.md#errors)).
- **The runtime doesn't retry an rpc on its own,** because it can't know whether the server already ran
  the handler. The caller decides, usually by showing a retry action.

> **Open question.** The client's rpc timeout isn't specified yet: its default, whether a protocol or an
> rpc can set its own, and whether it covers a streaming rpc beyond the call that opens the stream. Nor
> is how a test exercises a page's handling of `offline` and `timeout`; today it fakes the client
> service that wraps the protocol. See `docs/design/28-rpc-wire-format.md`.

## Streaming

Server streaming, an rpc that returns `Stream<T>`, works in browsers. The handler returns a stream and the
client reads it with `for await`; see [Server streaming](streaming.md). Client streaming and bidirectional
streaming are allowed only between servers, because browser `fetch` can't do them. Using them from
client code is a compile error.

## The wire format

Every protocol has **deterministic, locked Protobuf names**, so a non-K client, or `curl` in an
end-to-end test, can call an rpc without reading generated files:

| K | Protobuf / Connect |
|---|---|
| module `store` in project `shop` | package `shop.store` |
| `protocol StoreProtocol` | service `StoreProtocol` |
| `rpc placeOrder(request: OrderRequest)` | method `PlaceOrder` |
| URL | `POST /shop.store.StoreProtocol/PlaceOrder` |

- The project prefix is the Protobuf form of `k.toml`'s `name`: lower case, with `-`, `/` and `.` mapped to
  `_`. The names are always derived, and the serving app's `rpc.lock` records them, so renaming the
  project, the protocol, an rpc or its module fails `k check` (`K8001`): callers already deployed still
  use the old path. Mark the protocol `@Breaking("reason")` when that is acceptable.
- **Each rpc has a request message whose fields are its parameters,** named by each parameter's internal
  name: `func reviews(for product: ProductId)` takes `{"product": …}`. Changing an external label is
  source-breaking but wire-compatible.
- **A struct return type is the response message itself;** other return types are wrapped as
  `{"value": …}`, and an rpc without a return type answers `{}`.
- Servers accept JSON (`application/json`) and binary Protobuf (`application/proto`), and nothing else.
  Request bodies are limited to 1 MB, and every rpc response carries `Cache-Control: private, no-store`.

```bash
curl --header 'Content-Type: application/json' --header 'Connect-Protocol-Version: 1' \
     --data '{"request":{"productSlug":"k-mug","quantity":2,"email":"ana@example.com"}}' \
     http://localhost:8080/shop.store.StoreProtocol/PlaceOrder
```

### Status codes

| Outcome | Connect code | HTTP status |
|---|---|---|
| a declared error case | `failed_precondition`, with the case and its payload in the details | 400 |
| `rpcFailure(.invalid(…))` | `invalid_argument`, with field paths and messages | 400 |
| `rpcFailure(.rejected(.unauthenticated))` | `unauthenticated` | 401 |
| `rpcFailure(.rejected(.forbidden))` | `permission_denied` | 403 |
| `rpcFailure(.rejected(.rateLimited(retryAfter:)))` | `resource_exhausted`, with `Retry-After` | 429 |
| a request body over 1 MB | `resource_exhausted` | 413 |
| a content type other than JSON or Protobuf, or no `Connect-Protocol-Version` | — | 415 |
| a panic | `internal`, with no details outside development | 500 |
| `rpcFailure(.offline)`, `rpcFailure(.timeout)` | none: the client produces them, because nothing reached the server or came back | none |

Error types that cross the wire are `Codable`. The compiler warns when an error payload is a server-only
type or carries a third party's response body, with a hint to wrap the error without it.

> **Planned.** The first compiler releases speak JSON only. The binary Protobuf encoding, `k proto
> export` (which emits `.proto` files for non-K consumers) and `import proto "billing/v1/invoice.proto"`
> (which turns existing Protobuf definitions into K types) come later.

## Field numbers and `rpc.lock`

Protobuf compatibility depends on names and field numbers never changing. The compiler manages them for
you:

- Struct source stays clean. There are no `@Field(1)` annotations.
- The compiler assigns numbers and records them, with each protocol's wire service name and its rpcs, in
  the serving app's **`rpc.lock`**.
- **Breaking-change detection runs on every build.** A removed field, a changed type, a reused number or
  a changed wire name fails `k check`.
- A removed field's number is reserved automatically.
- A deliberate breaking change requires `@Breaking("reason")` on the protocol; `k build` then records
  the new shape. See [Workspaces and services](workspaces.md#rpclock-deploys-that-dont-break-callers).

> **Planned.** Bumping a protocol's version on a deliberate break (`CartProtocol.v2`, served next to the
> old one) isn't in the first compiler releases.

There is no separate `protocol.lock`: an earlier design kept the wire names in one, and `rpc.lock` now
records everything it would have.

Because K servers speak Connect, Go, Java and mobile clients can call them with ordinary gRPC or Connect
tooling.
