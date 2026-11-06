# Protocols and RPC

K is full-stack, and the browser talks to the server over **RPC, not REST**. A `protocol` declares the
contract in K. It compiles to **Protobuf** and runs over the **Connect protocol**, which works natively in
browsers and is wire-compatible with gRPC.

You never write a URL, a `fetch` call, a JSON parser or an API client. You call a function, and the
compiler checks both sides against the same contract.

## Declaring a protocol

```k
// CartProtocol.k: the contract
public protocol CartProtocol {
    rpc list() throws CartError -> List<CartItem> requires Authenticated

    rpc add(request: AddItemRequest) throws CartError -> CartItem requires Authenticated, RateLimit(100.perMinute)

    rpc watch(cartId: CartId) -> Stream<CartEvent>
}
```

Parameters and return types are ordinary `Codable` structs, enums, collections and core types.

> The design marks the position of the `requires` clause in an `rpc` signature as provisional. This book
> follows the compiler grammar, which places it after the complete signature, so the effects keep their
> fixed order: `async throws E -> T`.

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

    rpc watch(cartId: CartId) async -> Stream<CartEvent> { … }
}
```

- **A service that implements a protocol is server-only.** The compiler keeps it, and everything it
  injects, out of the client bundle.
- The compiler checks that every `rpc` is implemented, with matching types.
- An implementation may be `async` even though the contract doesn't say so: whether the server suspends
  is an implementation detail.

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
round trip.

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
} catch .rejected {
    message = "Please sign in again"
}
```

## Guards and middleware: `requires`

`requires` declares guards on a whole protocol or on individual rpcs, such as `Authenticated`, `Role(…)`
or `RateLimit(…)`. Guards are part of the contract, so clients can see them too. Cross-cutting
interceptors that never reject a call, such as logging and tracing, are configured once in `App.k`.

```k
public protocol NotesProtocol requires Authenticated {
    …
}
```

> **Proposal.** Guard rejections and `Session` follow `docs/design/proposals/auth-and-sessions.md`.

A guard's refusal is a typed error like any other. An rpc with a `requires` clause, its own or its
protocol's, must declare a `throws` type with a case whose only payload is `GuardRejection`, by convention
`case rejected(GuardRejection)`:

```k
public enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case rejected(GuardRejection)
}
```

`GuardRejection` is `.unauthenticated`, `.forbidden` or `.rateLimited(retryAfter:)`. Because the guard has
already run, `session.user` inside a guarded rpc is a `SessionUser`, not a `SessionUser?`.

## Streaming

Server streaming, an rpc that returns `Stream<T>`, works in browsers:

```k
rpc watch(cartId: CartId) -> Stream<CartEvent>
```

Client streaming and bidirectional streaming are allowed only between servers, because browser `fetch`
can't do them. Using them from client code is a compile error.

> **Proposal.** Producing and consuming streams (`for await`, `Broadcast<T>`, `@Scope(.server)`) follows
> `docs/design/proposals/server-streaming.md`.

## Field numbers and `protocol.lock`

Protobuf compatibility depends on field numbers never changing. The compiler manages them for you:

- Struct source stays clean. There are no `@Field(1)` annotations.
- The compiler assigns numbers and commits them to **`protocol.lock`**.
- **Breaking-change detection runs on every build.** A removed field, a changed type, or a reused number
  fails `k check`.
- A removed field's number is reserved automatically.
- A deliberate breaking change requires `@Breaking("reason")` on the protocol, which bumps its version
  (`CartProtocol.v2`).

## The wire format

Messages are JSON in development, so traffic is readable in the browser's developer tools, and binary
Protobuf in production.

> **Planned.** The binary Protobuf wire format is deferred. Until it lands, protocols run as JSON over
> HTTP, using Connect-compatible paths and error codes.

> **Proposal.** Generated names, messages and HTTP status codes follow
> `docs/design/proposals/rpc-wire-format.md`. For example, `rpc placeOrder` of `protocol StoreProtocol`
> in module `store` of package `shop` is served at `POST /shop.store.StoreProtocol/PlaceOrder`.

## Interop with other languages

Because K servers speak Connect, Go, Java and mobile clients can call them with ordinary gRPC or Connect
tooling.

> **Planned.** `k proto export`, which emits `.proto` files for non-K consumers, and
> `import proto "billing/v1/invoice.proto"`, which turns existing Protobuf definitions into K types, are
> deferred along with the binary wire format.

## Protocols and interfaces

K's `protocol` means a **wire contract** between processes, not Swift's protocols. In-process
abstractions use [`interface`](../language/declarations.md#interfaces).
