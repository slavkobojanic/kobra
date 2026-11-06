# Errors

K uses **typed, checked errors**. A function declares exactly what it can throw, every call that can
throw is marked with `try`, and `catch` must handle every case. Failures are part of a function's
signature, where readers and the compiler can see them.

## Declaring errors

An error type is an ordinary enum that implements `Error`:

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}
```

## Throwing

A function that can fail declares its error type with `throws`, and fails with `throw`:

```k
func reserve(product: Product, quantity: Int) throws CartError -> Reservation {
    guard product.stock >= quantity else {
        throw CartError.outOfStock(available: product.stock)
    }

    return Reservation(product: product.id, quantity: quantity)
}
```

`throws` always names exactly one type. There is no untyped `throws` and no `throws A | B`.

## Calling with `try`

**Every call that can throw is marked with `try`**, so a reader sees each point where execution can
jump:

```k
func add(product: Product) async throws CartError {
    let item = try await api.add(request: AddItemRequest(productId: product.id))
    items.append(item)
}
```

Inside a function that throws `CartError`, a `try` on a call that also throws `CartError` simply passes
the error on to the caller.

## Handling errors with `do` and `catch`

```k
func add() async {
    do {
        try await cart.add(product: product)
    } catch .outOfStock(let available) {
        error = .outOfStock(available: available)
    } catch .productNotFound, .unauthorized {
        Router.navigate(to: .home)
    }
}
```

- `catch` clauses use the same patterns as [`match`](control-flow.md#patterns): cases, bindings with
  `let`, several patterns in one clause, and `where` guards.
- **`catch` is exhaustive.** A function must handle every case of the error enum, or declare that it
  throws the same type so the unhandled cases pass through.
- **A case written without its payload matches any payload:** `catch .rpcFailure` handles every
  `rpcFailure(…)`, whatever it carries.

### Catching everything else

`catch let error` binds the whole error and matches every case not handled by an earlier clause. It lets
a caller handle a few cases on their own and the rest together, and it must come last:

```k
func refresh() async {
    do {
        forecast = try await weather.forecast(place: place, units: units)
    } catch .offline {
        problem = "You're offline. The forecast will refresh when you reconnect."
    } catch .upstream(.serviceUnavailable) {
        problem = "The weather service is down for maintenance."
    } catch let error {
        problem = "The forecast couldn't be loaded ({error})."      // error is a WeatherError
    }
}
```

- The bound error has the static type of what the `do` block throws (here `WeatherError`), never an
  untyped `Error`.
- Patterns nest: `.upstream(.serviceUnavailable)` matches one `HttpStatus` inside the case, because
  `HttpStatus` is an enum too ([Calling HTTP APIs](../apps/http.md#httpstatus)).
- **Each catch-all has one job.** `catch let error` binds; `catch _` doesn't. A `catch let x` whose
  binding is never used is a compile error with a quick fix to `catch _`.
- Like `default` in `match`, a catch-all is a **compile error when unreachable**, meaning every case is
  already handled. `catch` has no `default`; `catch _` takes its place.

## Wrapping at layer boundaries

**Errors are wrapped, not unioned.** A service that calls two protocols declares its own error enum and
wraps the inner errors in its own cases:

```k
enum CheckoutError implements Error {
    case emptyCart
    case payment(PaymentError)
    case inventory(InventoryError)
}

func checkout(card: CardToken) async throws CheckoutError -> Receipt {
    guard cart.items.isNotEmpty else { throw CheckoutError.emptyCart }

    do {
        try await inventory.reserve(items: cart.items)
    } catch let error {
        throw CheckoutError.inventory(error)          // error is an InventoryError
    }

    do {
        return try await payments.charge(amount: cart.total, card: card)
    } catch let error {
        throw CheckoutError.payment(error)            // error is a PaymentError
    }
}
```

Each layer speaks in terms that make sense at its level. A page that checks out deals with
`CheckoutError`, not with every error of every system behind it.

A boundary can also map each inner case to a case of its own. `WeatherService` does this with the
`HttpError` its HTTP client throws:

```k
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
```

## Turning errors into values

Sometimes you don't want to handle an error case by case. K uses keywords for this, never symbols.

`try … else` provides a fallback block, which must exit or produce a value:

```k
let item = try cart.add(product: product) else { return }
```

`Result.of` captures success or failure as a `Result<T, E>` value:

```k
let outcome = Result.of { try cart.add(product: product) }
```

There is no `try?` and no `!`.

## Functions that pass errors through

A function can take a block and throw whatever the block throws. Its parameter's type names a generic
error, `E`:

```k
// in k.db
func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T
```

- **A parameter typed `() async throws E -> T` makes the call `throws E`,** with `E` inferred from the
  block passed.
- **A block that can't throw makes `E` empty,** so the call can't throw either and needs no `try`:
  `await database.transaction { … }`.
- This is the only mechanism for rethrowing. A function that doesn't declare a generic error can't pass
  one through.

## Effects and event handlers handle their own errors

`@Effect` functions and event handlers can't be declared `throws`, because nothing calls them that could
catch the error. They handle every error inside, usually by recording it in state:

```k
@Effect(on: [userId])
func loadUser() async {
    do {
        profile = .loaded(try await users.load(userId))
    } catch let error {
        profile = .failed(error)
    }
}
```

Declaring `throws` on an effect, or passing a throwing function to `.on(…)`, is a compile error with a
quick fix that wraps the body in `do`/`catch`.

## Panics

Programmer bugs aren't errors. **An index out of range, a failed `assert`, or an impossible state
panics.** Panics are separate from errors and can't be caught with `catch`:

- In a view, a panic is caught by the nearest `ErrorBoundary`, which renders a fallback instead of the
  broken subtree.
- On the server, a panic becomes a `500` response with a trace in the logs.
- **Infrastructure failures on the server panic too:** a lost database connection, a full disk. They
  aren't outcomes the calling code could handle, so repository calls don't declare them. Inside a
  background job, a panic counts as a failed attempt and is retried.
- **A network failure on the client is not a panic.** An rpc that can't reach the server, or times out,
  throws `rpcFailure(.offline)` or `rpcFailure(.timeout)`, and an external API call throws
  `HttpError.offline` or `.timeout`. Losing the connection is a normal condition on the web, so the page
  handles it.

Use errors for things that can legitimately go wrong (the product sold out, the network is down), and
let panics expose bugs.

## Errors across the network

Typed errors cross the wire. When a protocol's `rpc` declares `throws CartError`, the client receives
exactly that error type, so client-side `catch` stays exhaustive over server errors.

An rpc can also fail outside its handler: a guard refuses the call, a request fails validation, or, in
the browser, the server can't be reached or doesn't answer in time. Those failures arrive in one case
with a fixed name, `rpcFailure(RpcFailure)`. Every rpc's error enum declares it, and an rpc with no
errors of its own declares `throws RpcOnlyError`:

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case rpcFailure(RpcFailure)
}

do {
    try await cart.add(product: product)
} catch .outOfStock(let available) {
    problem = .outOfStock(available: available)
} catch .rpcFailure(.rejected(.unauthenticated)) {
    Router.navigate(to: .signIn.returnTo(.cart))
} catch let other {
    problem = other
}
```

See [Protocols and RPC](../apps/protocols.md#failures-outside-the-handler).

## Testing errors

`expect throws` checks that a block throws a specific case. It takes a pattern, as `catch` does, so
nested cases can be checked (`expect throws AccessError.rpcFailure(.rejected(.unauthenticated))`), and a
case written without its payload matches any payload:

```k
test "out of stock surfaces a typed error" {
    expect throws CartError.outOfStock {
        try await cart.add(product: .soldOut)
    }
}
```

## Why not exceptions?

Unchecked exceptions hide failures: any call might throw anything, and nothing tells you. Java's checked
exceptions made failures visible but are disliked for their boilerplate. K keeps the safety and removes
most of the boilerplate with typed error enums, exhaustive `catch` patterns, and wrapping at layer
boundaries.
