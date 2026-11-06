# Errors

## Decision

K uses **typed, checked throws**, with a `try` marker at every call that can throw.

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

service CartService {
    @Inject let api: CartProtocol

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}

component AddToCart {
    @Inject let cart: CartService
    @State var error: CartError? = nil

    func add() async {
        do {
            try await cart.add(product: product)
        } catch .outOfStock(let available) {
            error = .outOfStock(available: available)
        } catch .productNotFound, .unauthorized {
            Router.navigate(to: .home)
        }
    }
}
```

## Rules

- **A function declares exactly what it throws:** `throws CartError`. The error type is an ordinary enum.
- **Every call that can throw is marked with `try`,** so a reader sees each point where execution can jump.
- **`catch` is exhaustive** over the error enum, with the same pattern rules as `match`. A function must handle every case or declare it in its own `throws` clause.
- **Errors are wrapped at layer boundaries, not unioned.** A service that calls two protocols declares its own error enum and wraps the inner errors (`case network(RpcError)`). There is no `throws A | B | C`.
- **Converting to values uses keywords:** `let item = try cart.add(...) else { return }`, or `Result.of { try cart.add(...) }`. There is no `try?` and no `!`.

## Panics

Programmer bugs (an index out of range, a failed `assert`, unwrapping `nil`) **panic**. Panics are separate from errors and can't be caught as regular errors.

- In a view, a panic hits the nearest `ErrorBoundary`.
- On the server, it becomes a 500 response with a trace.

## Why not Java's checked exceptions?

Java's checked exceptions are disliked mostly because of boilerplate. Typed error enums, exhaustive `catch` patterns and wrapping at layer boundaries keep the safety while removing most of it.
