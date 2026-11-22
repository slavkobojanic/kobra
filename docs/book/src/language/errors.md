# Errors

Kobra uses **typed, checked errors**. A function declares exactly what it can throw, every call that can
throw is marked with `try`, and `catch` must handle every case. Failures are part of a function's
signature, where readers and the compiler can see them.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/shop/Context.k
import k.http.HttpClient
import k.http.HttpStatus

struct ProductId implements Equatable, Codable {
    let value: Int
}

struct Product {
    let id: ProductId
    let stock: Int

    static let soldOut = Product(id: ProductId(value: 1), stock: 0)
}

struct Reservation {
    let product: ProductId
    let quantity: Int
}

struct AddItemRequest implements Codable {
    let productId: ProductId
}

struct CartItem {
    let productId: ProductId
}

struct Cart {
    let items: List<CartItem>
    let total: Money
}

interface CartApi {
    func add(request: AddItemRequest) async throws CartError -> CartItem
}

interface CartActions {
    func add(product: Product) async throws CartError
}

page Home at "/" {
    view {
        text { "Home" }
    }
}

page SignIn at "/sign-in" {
    @Param let returnTo: Page = .home

    view {
        text { "Sign in" }
    }
}

page CartPage at "/cart" {
    view {
        text { "Cart" }
    }
}

struct Place implements Codable {
    let name: String
}

enum UnitSystem {
    case metric
    case imperial
}

struct Forecast implements Codable {
    let summary: String
}

enum WeatherError implements Error {
    case offline
    case timeout
    case upstream(HttpStatus)
    case invalidResponse(reason: String)
}

interface Weather {
    func forecast(place: Place, units: UnitSystem) async throws WeatherError -> Forecast
}

struct CardToken {
    let value: String
}

struct Receipt {
    let total: Money
}

enum PaymentError implements Error {
    case declined
}

enum InventoryError implements Error {
    case soldOut
}

interface Inventory {
    func reserve(items: List<CartItem>) async throws InventoryError
}

interface Payments {
    func charge(amount: Money, card: CardToken) async throws PaymentError -> Receipt
}

struct User {
    let name: String
}

enum UserError implements Error {
    case notFound
}

interface Users {
    func load(_ id: Int) async throws UserError -> User
}
```
-->

## Declaring errors

An error type is an ordinary enum that implements `Error`:

```k
// src/shop/CartError.k
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
~struct CartModel {
~    let api: CartApi
~    var items: List<CartItem>
func add(product: Product) async throws CartError {
    let item = try await api.add(request: AddItemRequest(productId: product.id))
    items.append(item)
}
~}
```

Inside a function that throws `CartError`, a `try` on a call that also throws `CartError` simply passes
the error on to the caller.

## Handling errors with `do` and `catch`

```k
~import k.ui.Router
~
~component AddButton {
~    @Prop let cart: CartActions
~    @Prop let product: Product
~    @State var error: CartError? = nil
func add() async {
    do {
        try await cart.add(product: product)
    } catch .outOfStock(let available) {
        error = .outOfStock(available: available)
    } catch .productNotFound, .unauthorized {
        Router.navigate(to: .home)
    }
}
~    view {
~        button { "Add" }
~    }
~}
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
~component ForecastView {
~    @Prop let weather: Weather
~    @Prop let place: Place
~    @Prop let units: UnitSystem
~    @State var forecast: Forecast? = nil
~    @State var problem: String? = nil
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
~    view {
~        text { problem ?? "" }
~    }
~}
```

- The bound error has the static type of what the `do` block throws (here `WeatherError`), never an
  untyped `Error`.
- Patterns nest: `.upstream(.serviceUnavailable)` matches one `HttpStatus` inside the case, because
  `HttpStatus` is an enum too ([Calling HTTP APIs](../apps/http.md#httpstatus)).
- **Each catch-all has one job.** `catch let error` binds; `catch _` doesn't. A `catch let x` whose
  binding is never used is a compile error (`K2304`) with a quick fix to `catch _`.
- Like `default` in `match`, a catch-all is a **compile error when unreachable**, meaning every case is
  already handled. `catch` has no `default`; `catch _` takes its place.

### Catching cases added later: `catch unknown`

A list that names every case breaks when the error enum gains one: the new case isn't handled, so the
`do` is an error (`K2206`) until each list is updated. `catch _` avoids that, but then a new case is
handled silently, and nobody is told. **`catch unknown`** is the catch-all in between: write it after
clauses that name every case, and it runs only for a case they don't name.

```k
enum SaveError implements Error {
    case offline
    case conflict
}

~func save() throws SaveError {}
~
func saveNow() -> String {
    do {
        try save()
        return "Saved."
    } catch .offline {
        return "You're offline."
    } catch .conflict {
        return "Someone else changed it."
    } catch unknown {
        return "Couldn't save."
    }
}
```

- **It is never unreachable,** even though every case is named: it waits for one added later.
- **Once a case is added, the program still builds,** and the new case runs `catch unknown`'s block.
  The compiler warns (`K2207`) at each such `catch unknown`, listing the cases its clauses no longer
  name, with a fix that inserts them; `k check` fails on the warning, so CI finds every list.
- It binds nothing and takes no guard, and it is the last clause. `unknown` is a keyword only right
  after `catch`, before the block, so `.unknown` stays a case name.
- Use it where a list of every case is worth keeping, such as an error shown to the user case by case.
  `match` has no counterpart: a new case of a value enum usually needs new behavior, which `K2301`
  asks for.

## Wrapping at layer boundaries

**Errors are wrapped, not unioned.** A service that calls two protocols declares its own error enum and
wraps the inner errors in its own cases:

```k
enum CheckoutError implements Error {
    case emptyCart
    case payment(PaymentError)
    case inventory(InventoryError)
}

~struct CheckoutService {
~    let cart: Cart
~    let inventory: Inventory
~    let payments: Payments
~
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
~}
```

Each layer speaks in terms that make sense at its level. A page that checks out deals with
`CheckoutError`, not with every error of every system behind it.

A boundary can also map each inner case to a case of its own. `WeatherService` does this with the
`HttpError` its HTTP client throws:

```k
~import k.http.HttpClient
~import k.http.HttpRequest
~
~struct WeatherService {
~    let http: HttpClient
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
~}
```

## Turning errors into values

Sometimes you don't want to handle an error case by case. Kobra uses keywords for this, never symbols.

`try … else` provides a fallback block, which must exit or produce a value:

```k
~enum AddError implements Error {
~    case soldOut
~}
~
~struct LocalCart {
~    func add(product: Product) throws AddError -> CartItem {
~        return CartItem(productId: product.id)
~    }
~}
~
~func example(cart: LocalCart, product: Product) {
let item = try cart.add(product: product) else { return }
~}
```

`Result.of` captures success or failure as a `Result<T, E>` value:

```k
~enum AddError implements Error {
~    case soldOut
~}
~
~struct LocalCart {
~    func add(product: Product) throws AddError -> CartItem {
~        return CartItem(productId: product.id)
~    }
~}
~
~func example(cart: LocalCart, product: Product) {
let outcome = Result.of { try cart.add(product: product) }
~}
```

There is no `try?` and no `!`.

## Functions that pass errors through

A function can take a block and throw whatever the block throws. Its parameter's type names a generic
error, `E`:

```k,ignore
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
~component Profile {
~    @Prop let users: Users
~    @Prop let userId: Int
~    @State var profile: LoadState<User> = .idle
@Effect(on: [userId])
func loadUser() async {
    do {
        profile = .loaded(try await users.load(userId))
    } catch let error {
        profile = .failed(error)
    }
}
~    view {
~        text { "Profile" }
~    }
~}
```

Declaring `throws` on an effect (`K3012`), or passing a throwing function to `.on(…)` (`K3017`), is a
compile error: wrap the body in `do`/`catch` and record the failure.

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

```k,standalone
~import k.ui.Router
~import k.rpc.RpcFailure
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case rpcFailure(RpcFailure)
}

~struct Product {
~    let name: String
~}
~
~interface CartApi {
~    func add(product: Product) async throws CartError
~}
~
~page Home at "/" {
~    view {
~        text { "Home" }
~    }
~}
~
~page SignIn at "/sign-in" {
~    @Param let returnTo: Page = .home
~
~    view {
~        text { "Sign in" }
~    }
~}
~
~page Cart at "/cart" {
~    view {
~        text { "Cart" }
~    }
~}
~
~component AddToCart {
~    @Prop let cart: CartApi
~    @Prop let product: Product
~    @State var problem: CartError? = nil
~
~    func add() async {
do {
    try await cart.add(product: product)
} catch .outOfStock(let available) {
    problem = .outOfStock(available: available)
} catch .rpcFailure(.rejected(.unauthenticated)) {
    Router.navigate(to: .signIn.returnTo(.cart))
} catch let other {
    problem = other
}
~    }
~
~    view {
~        text { "Cart" }
~    }
~}
```

See [Protocols and RPC](../apps/protocols.md#failures-outside-the-handler).

## Testing errors

`expect throws` checks that a block throws a specific case. It takes a pattern, as `catch` does, so
nested cases can be checked (`expect throws AccessError.rpcFailure(.rejected(.unauthenticated))`), and a
case written without its payload matches any payload:

```k
~struct LocalCart {
~    func add(product: Product) async throws CartError {
~        throw .outOfStock(available: product.stock)
~    }
~}
~
~let cart = LocalCart()
test "out of stock surfaces a typed error" {
    expect throws CartError.outOfStock {
        try await cart.add(product: .soldOut)
    }
}
```

## Why not exceptions?

Unchecked exceptions hide failures: any call might throw anything, and nothing tells you. Java's checked
exceptions made failures visible but are disliked for their boilerplate. Kobra keeps the safety and removes
most of the boilerplate with typed error enums, exhaustive `catch` patterns, and wrapping at layer
boundaries.
