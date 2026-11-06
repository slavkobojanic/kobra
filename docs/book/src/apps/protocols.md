# Protocols and RPC

K is full-stack, and the browser talks to the server over **RPC, not REST**. A `protocol` declares the
contract in K. It compiles to **Protobuf** and runs over the **Connect protocol**, which works natively in
browsers and is wire-compatible with gRPC.

You never write a URL, a `fetch` call, a JSON parser or an API client. You call a function, and the
compiler checks both sides against the same contract. RPC is the only way an app's own client talks to
its server; the one escape hatch to raw HTTP, for external systems, is the
[endpoint page](endpoints.md).

## Declaring a protocol

```k
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

    rpc watch(cartId: CartId) async throws CartError -> Stream<CartEvent> { … }
}
```

- **A service that implements a protocol is server-only.** The compiler keeps it, and everything it
  injects, out of the client bundle.
- The compiler checks that every `rpc` is implemented, with matching types.
- An implementation may be `async` even though the contract doesn't say so: whether the server suspends
  is an implementation detail.
- The protocol's `Authenticated` guard has already run inside `CartApi`, so `session.user` is a
  `SessionUser`, not an optional ([Authentication and sessions](auth.md)).

## Calling it from the client

**Only services inject protocols.** Components use them through services:

```k
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
```

## Guards and middleware: `requires`

`requires` declares guards on a whole protocol or on individual rpcs, such as `Authenticated`,
`Role(.admin)` or `RateLimit(10.perMinute)`. Guards are part of the contract, so clients can see them too.
Cross-cutting interceptors that never reject a call, such as logging and tracing, are configured once in
`App.k`.

```k
public protocol AdminProtocol {
    rpc signIn(request: SignInRequest) throws SignInError requires RateLimit(5.perMinute)

    rpc recentOrders() throws AdminError -> List<OrderSummary> requires Authenticated, Role(.admin)
}
```

## Failures outside the handler

An rpc call can fail before its handler runs: a guard refuses it, or a `Validatable` parameter fails
validation. Those failures are one standard enum in `k.rpc`:

```k
public enum RpcFailure implements Error {
    case rejected(GuardRejection)       // a guard refused the call
    case invalid(ValidationError)       // a Validatable parameter failed validation
}
```

**An rpc that can fail before its handler runs must declare an error enum with exactly one case
`rpcFailure(RpcFailure)`.** The name is fixed, so the runtime never has to find the case by its shape:

```k
public enum CheckoutError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case paymentUnavailable
    case rpcFailure(RpcFailure)
}

public protocol StoreProtocol {
    rpc placeOrder(request: OrderRequest) throws CheckoutError -> PlacedOrder requires RateLimit(10.perMinute)
}
```

- A guarded or validated rpc without that case is a compile error, with a quick fix that adds it.
- **An rpc with no other errors declares `throws RpcOnlyError`,** a standard enum whose only case is
  `rpcFailure(RpcFailure)`.
- **Only the runtime produces `rpcFailure`.** A handler can't throw it. Client code that wraps errors at
  a layer boundary passes it on like any other case:

  ```k
  catch .rpcFailure(let failure) {
      throw LookupError.rpcFailure(failure)
  }
  ```

- `GuardRejection` is `.unauthenticated`, `.forbidden` or `.rateLimited(retryAfter: Duration)`.
- A `@Form` shows `.invalid` under its fields when the submit action rethrows the `ValidationError`
  ([Forms and validation](forms.md)).

> **Open question.** What a browser sees when it can't reach the server during an rpc (offline, or the
> request timed out) isn't decided. The recommended design adds `case offline` and `case timeout` to
> `RpcFailure`, so every rpc called from the client declares an error enum; the alternative is a panic
> that an `OfflineBoundary` catches. See `docs/design/28-rpc-wire-format.md`.

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
  `_`. The names are recorded in `protocol.lock` when first derived, so renaming the project, the
  protocol, an rpc or its module keeps the wire names.
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

Error types that cross the wire are `Codable`. The compiler warns when an error payload is a server-only
type or carries a third party's response body, with a hint to wrap the error without it.

> **Planned.** The first compiler releases speak JSON only. The binary Protobuf encoding, `k proto
> export` (which emits `.proto` files for non-K consumers) and `import proto "billing/v1/invoice.proto"`
> (which turns existing Protobuf definitions into K types) come later.

## Field numbers and `protocol.lock`

Protobuf compatibility depends on names and field numbers never changing. The compiler manages them for
you:

- Struct source stays clean. There are no `@Field(1)` annotations.
- The compiler assigns numbers and records them, with each protocol's package, service and method names,
  in **`protocol.lock`**.
- **Breaking-change detection runs on every build.** A removed field, a changed type, a reused number or
  a changed wire name fails `k check`.
- A removed field's number is reserved automatically.
- A deliberate breaking change requires `@Breaking("reason")` on the protocol, which bumps its version
  (`CartProtocol.v2`).

Because K servers speak Connect, Go, Java and mobile clients can call them with ordinary gRPC or Connect
tooling.
