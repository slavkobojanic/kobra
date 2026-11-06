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
- **An rpc's error enum carries the failures that happen outside its handler** (a refused guard, an invalid request, and in the browser being offline or timing out) in one case with a fixed name, `rpcFailure(RpcFailure)`. Every rpc has it; an rpc with no errors of its own declares `throws RpcOnlyError`. See [28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler).

## Catching everything that remains

**`catch let name` binds the whole thrown value** and matches every case not handled by an earlier clause. It lets a caller handle a few cases on their own and the rest together:

```k
// WeatherService: the layer boundary maps each HttpError case to a WeatherError case
private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    do {
        return try await http.fetchJson(request: HttpRequest(method: .get, url: url).query(query))
    } catch .offline {
        throw WeatherError.offline
    } catch .timeout {
        throw WeatherError.timeout
    } catch .status(let status, _) {
        throw WeatherError.upstream(status)          // the third party's body is dropped
    } catch .decoding(let message) {
        throw WeatherError.invalidResponse(reason: message)
    }
}

// A component: two cases get their own message, the rest share one
func refresh() async {
    do {
        forecast = try await weather.forecast(place: place, units: units)
    } catch .offline {
        problem = "You're offline. The forecast will refresh when you reconnect."
    } catch .upstream(.serviceUnavailable) {
        problem = "The weather service is down for maintenance."
    } catch let error {
        problem = "The forecast couldn't be loaded ({error})."      // error: WeatherError
    }
}
```

- `name` has the static error type of the `try` expressions in the `do` block (here `WeatherError`), never an untyped `Error`.
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
- **A network failure on the client is not a panic.** An rpc that can't reach the server, or times out, throws `rpcFailure(.offline)` or `rpcFailure(.timeout)` ([28-rpc-wire-format.md](28-rpc-wire-format.md#network-failures)), and an external API call throws `HttpError.offline` or `.timeout` ([32-http-client.md](32-http-client.md#errors)). Losing the connection is a normal condition on the web, so the page handles it.

## Why not Java's checked exceptions?

Java's checked exceptions are disliked mostly because of boilerplate. Typed error enums, exhaustive `catch` patterns and wrapping at layer boundaries keep the safety while removing most of it.
