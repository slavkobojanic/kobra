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

### Catching everything else

> **Proposal.** `catch let` follows `docs/design/proposals/catch-binding.md`.

`catch let error` binds the whole error and matches every case not handled by an earlier clause. It must
come last:

```k
do {
    return try await http.fetchJson(request: request)
} catch .decoding(let message) {
    throw WeatherError.invalidResponse(reason: message)
} catch let error {
    throw WeatherError.network(error)            // error is an HttpError
}
```

The bound error has the static type of what the `do` block throws, never an untyped `Error`.

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

## Panics

Programmer bugs aren't errors. **An index out of range, a failed `assert`, or an impossible state
panics.** Panics are separate from errors and can't be caught with `catch`:

- In a view, a panic is caught by the nearest `ErrorBoundary`, which renders a fallback instead of the
  broken subtree.
- On the server, a panic becomes a `500` response with a trace in the logs.

Use errors for things that can legitimately go wrong (the product sold out, the network is down), and
let panics expose bugs.

## Errors across the network

Typed errors cross the wire. When a protocol's `rpc` declares `throws CartError`, the client receives
exactly that error type, so client-side `catch` stays exhaustive over server errors. See
[Protocols and RPC](../apps/protocols.md).

## Testing errors

`expect throws` checks that a block throws a specific case:

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
