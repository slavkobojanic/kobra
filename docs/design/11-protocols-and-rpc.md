# Protocols and RPC

## Decision

K is full-stack, and the client talks to the server over **RPC, not REST**. A `protocol` declares the contract in K syntax. It compiles to **Protobuf** and runs over the **Connect protocol** (from Buf).

Why Connect:

- It works natively in browsers over HTTP/1.1 and HTTP/2. gRPC needs a proxy.
- It is wire-compatible with gRPC and gRPC-Web, so Go, Java and mobile clients can talk to a K server.
- Messages are binary Protobuf in production and JSON in development, so traffic is readable in devtools.

## Example

```k
// CartProtocol.k: the contract
protocol CartProtocol {
    rpc list() requires Authenticated -> List<CartItem>

    rpc add(request: AddItemRequest) requires Authenticated, RateLimit(100.perMinute) throws CartError -> CartItem

    rpc watch(cartId: CartId) -> Stream<CartEvent>
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

> The exact position of the `requires` clause in an `rpc` signature is *provisional*.

## Rules

- **Only services inject protocols.** Components consume them through services. See [10-services.md](10-services.md).
- **Injection works the same regardless of location.** `@Inject let api: CartProtocol` gets a generated network client in the browser and a **direct in-process call during server rendering**, with no HTTP round trip.
- **A service that implements a protocol is server-only.** The compiler keeps it, and everything it injects, out of the client bundle.
- **Typed errors cross the wire.** `throws CartError` maps to Connect error details, so client-side `catch` stays exhaustive over server errors.
- **`requires` declares guards and middleware** on protocols and on individual `rpc`s (for example `Authenticated` or `RateLimit`). They are visible to clients too. Cross-cutting interceptors that never reject a call (logging, tracing) are configured once in `App.k`.
- **Streaming:**
  - Server streaming (`Stream<T>`) works in browsers.
  - Client streaming and bidirectional streaming are allowed only between servers, because browser `fetch` can't do them. Using them from client code is a compile error.
- **Naming:** K's `protocol` means a wire contract, not Swift's protocols. In-process abstractions use `interface`.

## Interop

- `k proto export` emits `.proto` files for non-K consumers.
- `import proto "billing/v1/invoice.proto"` turns existing protos into K types.

## Field numbers: `protocol.lock`

Protobuf compatibility depends on field numbers never changing. The compiler manages them:

- Struct source stays clean: no `@Field(1)` annotations.
- The compiler assigns numbers and commits them to **`protocol.lock`**.
- **Breaking-change detection runs on every build:** a removed field, a changed type, or a reused number fails `k check`.
- A removed field's number is reserved automatically.
- A deliberate breaking change requires `@Breaking("reason")` on the protocol, which bumps its version (`CartProtocol.v2`).
