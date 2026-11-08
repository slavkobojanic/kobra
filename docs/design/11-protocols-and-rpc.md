# Protocols and RPC

## Decision

K is full-stack, and the client talks to the server over **RPC, not REST**. A `protocol` declares the contract in K syntax. It compiles to **Protobuf** and runs over the **Connect protocol** (from Buf).

Why Connect:

- It works natively in browsers over HTTP/1.1 and HTTP/2. gRPC needs a proxy.
- It is wire-compatible with gRPC and gRPC-Web, so Go, Java and mobile clients can talk to a K server.
- Messages are binary Protobuf in production and JSON in development, so traffic is readable in devtools.

RPC is the only way the app's own client talks to its server. The one deliberate escape hatch to raw HTTP is the endpoint page, for external systems that can't speak RPC ([27-endpoint-pages.md](27-endpoint-pages.md)).

## Example

```k
// CartProtocol.k: the contract
protocol CartProtocol requires Authenticated {
    rpc list() throws CartError -> List<CartItem>

    rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)

    rpc watch(cartId: CartId) throws CartError -> Stream<CartEvent>
}

// Server implementation
service CartApi implements CartProtocol {
    @Inject let repo: CartRepository
    @Inject let session: Session

    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        guard let product = await repo.product(request.productId) else {
            throw CartError.productNotFound
        }

        return await repo.add(product, for: session.user.id)
    }
}

// Client side: a service wraps the protocol
service CartService {
    @Inject let api: CartProtocol

    @State var items: List<CartItem> = []

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}
```

`CartError` has a case `rpcFailure(RpcFailure)`, which every rpc's error enum requires: it carries guard refusals, invalid requests and, in the browser, network failures ([28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler)). An rpc with no errors of its own declares `throws RpcOnlyError`. Inside `CartApi`, the protocol's `Authenticated` guard has already run, so `session.user` is a `SessionUser`, not an optional ([26-auth-and-sessions.md](26-auth-and-sessions.md#narrowing-under-authenticated)).

## Rules

- **The signature reads in a fixed order:** `rpc name(params) async throws E -> T requires Guards`. `requires` comes after the complete signature, as in `page` and `protocol` headers, so the effects `async throws E -> T` stay uninterrupted.
- **Only services inject protocols.** Components consume them through services. See [10-services.md](10-services.md).
- **Injection works the same regardless of location.** `@Inject let api: CartProtocol` gets a generated network client in the browser and a **direct in-process call during server rendering**, with no HTTP round trip. Guards and validation still run on in-process calls.
- **A service that implements a protocol is server-only.** The compiler keeps it, and everything it injects, out of the client bundle.
- **Typed errors cross the wire.** `throws CartError` maps to Connect error details, so client-side `catch` stays exhaustive over server errors. **Every rpc declares a `throws` clause.** The status codes, the message names and the failure cases are specified in [28-rpc-wire-format.md](28-rpc-wire-format.md).
- **`requires` declares guards and middleware** on protocols and on individual `rpc`s (for example `Authenticated` or `RateLimit`). They are visible to clients too. Cross-cutting interceptors that never reject a call (logging, tracing) are configured once in `App.k`.
- **Streaming:**
  - Server streaming (`Stream<T>`) works in browsers. How a handler produces a stream and how a client reads it is in [29-server-streaming.md](29-server-streaming.md).
  - Client streaming and bidirectional streaming are allowed only between servers, because browser `fetch` can't do them. Using them from client code is a compile error.
- **Naming:** K's `protocol` means a wire contract, not Swift's protocols. In-process abstractions use `interface`.

## Interop

- `k proto export` emits `.proto` files for non-K consumers.
- `import proto "billing/v1/invoice.proto"` turns existing protos into K types.

## Field numbers and `rpc.lock`

Protobuf compatibility depends on names and field numbers never changing. The compiler manages them:

- Struct source stays clean: no `@Field(1)` annotations.
- The compiler assigns field numbers and records them, together with each protocol's wire service name ([28-rpc-wire-format.md](28-rpc-wire-format.md#names)), its rpcs and their error cases, in the serving app's **`rpc.lock`** ([40-workspaces-and-services.md](40-workspaces-and-services.md#compatibility-between-deploys)).
- **Breaking-change detection runs on every check:** a removed field, rpc or case, a changed type, a reused number or a changed wire name fails `k check` (`K8001`).
- A removed field's number is reserved automatically.
- A deliberate breaking change requires `@Breaking("reason")` on the protocol; `k build` then records the new shape. Bumping the protocol's version on such a change (`CartProtocol.v2`, served beside the old one) is designed but not built.

**Decision: there is no separate `protocol.lock`.** The design first gave the wire names and field numbers a `protocol.lock` of their own, in which the names, once derived, would be kept across renames. `rpc.lock` now records everything that lock was to protect, so it replaces it ([13-tooling.md](13-tooling.md#lockfiles)). Wire names are always derived from the current names, and a rename that changes one is reported like any other breaking change, rather than silently kept.
