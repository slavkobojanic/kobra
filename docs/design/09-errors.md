# Errors

## Decision

Kobra uses **typed, checked throws**, with a `try` marker at every call that can throw.

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
- **A `do` may call into two error types** (a device API's and an rpc's): each `catch` clause names a case of one of them and catches only that enum's case, even where both enums have a case at the same position. A catch-all over two types binds an `Error`, which can be passed on but not matched against a case (K2312): an enum value doesn't carry its enum, so name the cases in `catch` clauses instead.
- **An `Error` prints as its case name,** as a value of its own enum does: `"{error}"` is `offline` for `HttpError.offline` and `denied` for `DeviceError.denied(reason: "no camera")`, payloads left out. That holds for stdlib errors, and for an `Error` passed to a parameter, stored or listed. The text is taken where the concrete type is still known (the catch-all, or the conversion to `Error`), since the value alone doesn't say which enum it belongs to. An error type wants its own wording through a `switch` over its cases, not through `Error`.
- **It keeps that text wherever it goes:** a returned value, a field's or parameter's default, a branch of an `if`, and a generic function instantiated at `Error` all convert the concrete error as an argument does. A `throw error` of an `Error` rethrows the same value, so the catch upstream, which also sees an `Error` (a function `throws Error` throws only `Error` values), prints the same case. An `Error` has no codec: it can't cross the wire, alone or as the error of a `Result` (K5503), so wrap it in an rpc's own error enum instead.
- **Each catch-all has one job.** `catch let error` binds; `catch _` doesn't. A `catch let x` whose binding is never used is a compile error with a quick fix to `catch _`.
- It must be the last clause. Like `default` in `match`, it is a **compile error when unreachable**, meaning every case is already handled.
- `match` has the same pair: `case let other` binds, and `default` doesn't ([08-types.md](08-types.md#match)). `catch` has no `default`, so `catch _` takes its place.

## Cases added later: `catch unknown`

**Decision** (2026-11-11, from huddle: adding a case to an error enum broke every complete catch list at every call site). Error enums grow: a new failure of an rpc, a new `HttpError`. A `do` whose clauses name every case breaks when one is added (K2206), and the only alternative, `catch _`, takes every future case silently. **`catch unknown`** is the catch-all between the two, as Swift's `@unknown default` is for `switch`:

```k
do {
    try await notes.save(note)                  // throws SaveError: conflict, tooLarge, rpcFailure
} catch .conflict {
    problem = "Someone else changed this note."
} catch .tooLarge {
    problem = "The note is too long."
} catch .rpcFailure {
    problem = "Couldn't save. Check your connection."
} catch unknown {
    problem = "Couldn't save."                  // a case SaveError gains later
}
```

- **It runs for the cases the clauses before it don't name,** like `catch _`; it binds nothing, takes no guard and must be last.
- **It is never unreachable** (no K2302) after clauses that name every case: that is what it is for. After another catch-all it is K2302, as any clause is.
- **When its clauses don't name every case, it is a warning, K2207 `unknown-catch-not-exhaustive`,** at `unknown`, listing the missing cases, with a fix that inserts their clauses before it. A warning, not an error: the program builds, runs and handles the new case with `catch unknown`'s block, and `k check` (which fails on any problem) still finds every list in CI. A `do` the new case reaches without `catch unknown` is K2206 as before.
- **`unknown` is a keyword only between `catch` and `{`.** A bare name in a pattern was already K0115, so no program changes meaning; `.unknown` is still a case.
- **`match` keeps its rules.** A new case of a value enum usually needs new behavior, which K2301 asks for at each `match`; an error, unlike a value, has an obvious fallback (say it failed), and errors are what an outside party (a service in a workspace, the standard library) adds to.

Why: the compiler can't tell a case added later from one forgotten, but the person who wrote the clauses can say "every case I know of, and a fallback for the rest". That keeps the list checked against the enum (the warning names what it gained) without breaking builds when an enum another team or the toolchain owns gains a case, which [48](48-releases-and-versioning.md#open-questions) asked about for `RpcFailure` and `HttpError`. **Alternatives:** `@Extensible` error enums whose every catch needs a catch-all (Rust's `#[non_exhaustive]`): the enum's author would decide for every caller, the catch-all would hide new cases with no warning, and the app's own enums, where the friction was felt, gain nothing; allowing an unreachable `catch _` (new cases silently handled, the same as today's `catch _`); a warning instead of K2206 for every incomplete list (an error forgotten in a list would then reach the function's caller as nothing at all, since the `do` would have to catch it somehow); recording each enum's cases in a lockfile to tell new ones apart (only rpc errors have a lock).

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
