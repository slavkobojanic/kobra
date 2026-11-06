# Errors

## Decision

K uses **typed, checked throws**, with a `try` marker at every call that can throw.

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case rpcFailure(RpcFailure)
}

service CartService {
    @Inject let api: CartProtocol

    @State var items: List<CartItem> = []

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}

component AddToCart {
    @Inject let cart: CartService
    @Prop let product: Product
    @State var problem: CartError? = nil

    func add() async {
        do {
            try await cart.add(product: product)
        } catch .outOfStock(let available) {
            problem = .outOfStock(available: available)
        } catch .rpcFailure(.rejected(.unauthenticated)) {
            Router.navigate(to: .signIn.returnTo(.cart))
        } catch let other {
            problem = other
        }
    }
}
```

## Rules

- **A function declares exactly what it throws:** `throws CartError`. The error type is an ordinary enum.
- **Every call that can throw is marked with `try`,** so a reader sees each point where execution can jump.
- **`catch` is exhaustive** over the error enum, with the same pattern rules as `match`. A function must handle every case or declare it in its own `throws` clause. A case written without its payload (`catch .rpcFailure`) matches any payload.
- **Errors are wrapped at layer boundaries, not unioned.** A service that calls two protocols declares its own error enum and wraps the inner errors (`case payments(PaymentError)`). There is no `throws A | B | C`.
- **Converting to values uses keywords:** `let item = try cart.add(...) else { return }`, or `Result.of { try cart.add(...) }`. There is no `try?` and no `!`.
- **An rpc's error enum carries the failures that happen outside its handler** (a refused guard, an invalid request) in one case with a fixed name, `rpcFailure(RpcFailure)`. See [28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler).

## Catching everything that remains

**`catch let name` binds the whole thrown value** and matches every case not handled by an earlier clause. It is what wrapping at a layer boundary needs:

```k
private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    do {
        return try await http.fetchJson(request: HttpRequest(method: .get, url: url).query(query))
    } catch .decoding(let message) {
        throw WeatherError.invalidResponse(reason: message)
    } catch let error {
        throw WeatherError.network(error)            // error: HttpError
    }
}
```

- `name` has the static error type of the `try` expressions in the `do` block (here `HttpError`), never an untyped `Error`.
- **Each catch-all has one job.** `catch let error` binds; `catch _` doesn't. A `catch let x` whose binding is never used is a compile error with a quick fix to `catch _`.
- It must be the last clause. Like `default` in `match`, it is a **compile error when unreachable**, meaning every case is already handled.
- `match` has the same pair: `case let other` binds, and `default` doesn't ([08-types.md](08-types.md#match)). `catch` has no `default`, so `catch _` takes its place.

## Error-generic functions

A function can take a block whose error type it passes through:

```k
// in k.db
func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T
```

- **A parameter typed `() async throws E -> T` makes the call `throws E`,** with `E` inferred from the block passed.
- **A block that can't throw makes `E` empty,** so the call can't throw either and needs no `try`: `await database.transaction { … }`.
- This is the only mechanism for "rethrowing". Functions that don't declare a generic error can't pass one through.

## Effects and event handlers

`@Effect` functions and event handlers can't be declared `throws`, because nothing calls them that could catch the error. They handle every error inside, usually by recording it in state ([03-reactivity.md](03-reactivity.md), [04-components.md](04-components.md#handlers)).

## Panics

Programmer bugs (an index out of range, a failed `assert`, unwrapping `nil`) **panic**. Panics are separate from errors and can't be caught as regular errors.

- In a view, a panic hits the nearest `ErrorBoundary`.
- On the server, it becomes a 500 response with a trace.
- **Infrastructure failures on the server panic too:** a lost database connection, a full disk. They are not domain outcomes the calling code could handle, so repository calls don't declare them ([19-persistence.md](19-persistence.md)). Inside a background job, a panic counts as a failed attempt and is retried ([30-background-jobs.md](30-background-jobs.md)).
- Whether a network failure on the client (offline, timed out) during an rpc is a typed error or a panic is an **open question**, recorded in [28-rpc-wire-format.md](28-rpc-wire-format.md#open-question-network-failures-on-the-client).

## Why not Java's checked exceptions?

Java's checked exceptions are disliked mostly because of boilerplate. Typed error enums, exhaustive `catch` patterns and wrapping at layer boundaries keep the safety while removing most of it.
