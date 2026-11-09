# Control flow

K has four main control-flow constructs: `if`, `match`, `guard` and `for`. All of them work the same in
ordinary code and in [views](../apps/views.md).

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/shop/Context.k
struct CartItem {
    let id: Int
    var quantity: Int
}

struct Cart {
    let items: List<CartItem>
    let total: Money
}

enum Country {
    case norway
    case sweden
}

struct Address {
    let country: Country
}

struct User {
    let name: String
    let address: Address?
}

struct AppSession {
    let user: User?
}

enum Status {
    case active
    case paused
}

struct Tracking {
    let isDelayed: Bool
}

enum OrderStatus {
    case pending
    case paid
    case shipped(Tracking)
    case cancelled(reason: String)
}

struct Order {
    let status: OrderStatus
}

enum NoteError implements Error {
    case notFound
    case offline
}

enum CheckoutError implements Error {
    case notLoggedIn
    case emptyCart
    case declined
}

struct Receipt {
    let total: Money
}

interface Payments {
    func charge(_ user: User, _ amount: Money) async throws CheckoutError -> Receipt
}

struct NoteChange {
    let id: Int
}

interface NotesApi {
    func watch() async throws NoteError -> Stream<NoteChange>
}

component StatusBadge {
    @Prop let status: OrderStatus

    view {
        text { "status" }
    }
}

component DelayBanner {
    @Prop let tracking: Tracking

    view {
        text { "Delayed" }
    }
}

component TrackingLink {
    @Prop let tracking: Tracking

    view {
        text { "Track" }
    }
}

component CartRow {
    @Prop let item: CartItem
    @Binding var quantity: Int

    view {
        text { "{quantity}" }
    }
}
```
-->

## `if`

```k
~import k.observe.Log
~
~func review(cart: Cart, requireConfirmation: () -> Void, checkout: () -> Void) {
if cart.items.isEmpty {
    Log.info("nothing to check out")
} else if cart.total > 1000.usd {
    requireConfirmation()
} else {
    checkout()
}
~}
```

Conditions are plain `Bool` expressions with no parentheses around them. Combine them with `and`, `or`
and `not`.

### `if` is an expression

**K has no ternary operator.** `if` is an expression, so the same construct produces values:

```k
~func example(done: Bool, isCompact: Bool) {
let label = if done { "Done" } else { "Pending" }

let size: Length = if isCompact { 8px } else { 16px }
~}
```

When `if` is used as a value, each branch's last expression is its value, and an `else` is required.

### `if let`

`if let` unwraps an optional. Inside the block, the name refers to the unwrapped value:

```k
~component Welcome {
~    @Prop let session: AppSession
~    @Slot let footer: View?
~
~    func greet(user: User) {}
~
~    func start() {
if let user = session.user {
    greet(user: user)
}

~    }
~
~    view {
if let footer {                  // short for `if let footer = footer`
    footer
}
~    }
~}
```

Several conditions can be combined with commas. All must succeed, in order, and later conditions can use
names bound by earlier ones:

```k
~func example(session: AppSession, showNorwegianShipping: () -> Void) {
if let user = session.user, let address = user.address, address.country == .norway {
    showNorwegianShipping()
}
~}
```

There is no `if var` and no `if case`. To branch on enum cases, use `match`.

## `match`

`match` compares a value against patterns, and is **exhaustive**: every possible value must be handled.

```k
~func example(status: Status) {
let label = match status {
    case .active { "On" }
    case .paused { "Off" }
}
~}
```

Like `if`, `match` is an expression. Each arm's last expression is its value.

### Patterns

```k
~component OrderStatusView {
~    @Prop let order: Order
~
~    view {
match order.status {
    case .pending, .paid {                          // several patterns in one arm
        StatusBadge(status: order.status)
    }

    case .shipped(let tracking) where tracking.isDelayed {
        DelayBanner(tracking: tracking)             // a guard with `where`
    }

    case .shipped(let tracking) {
        TrackingLink(tracking: tracking)
    }

    case .cancelled(reason: let reason) {           // a labeled associated value
        paragraph { reason }
    }
}
~    }
~}
```

| Pattern | Matches |
|---|---|
| `.name`, `Type.name` | an enum case |
| `.name(let x)` | an enum case, binding its associated value |
| `.name(label: let x)` | an enum case, binding a labeled associated value |
| `42`, `"text"`, `true`, `nil` | a literal |
| `0..<10`, `1...5` | a value in a range |
| `(let a, let b)` | a tuple |
| `let x` | anything, binding it |
| `_` | anything |

Bindings always use `let`. A bare name in a pattern is a compile error, because it would be unclear
whether it binds a new name or compares against an existing one: write `let x` to bind or `.x` for an
enum case.

### `default`

```k
~func example(count: Int) {
let size = match count {
    case 0 { "empty" }
    case 1..<10 { "small" }
    default { "large" }
}
~}
```

- `default` is **required** when matching values that can't be enumerated: numbers, strings, ranges,
  tuples.
- `default` is **allowed** on enums, as a deliberate trade-off: new cases will fall into it silently.
- `default` is a **compile error when unreachable**, meaning every case is already covered.
- **`default` is the only non-binding catch-all.** A top-level `case _` arm is a compile error (`K2303`)
  with a quick fix to `default`. `_` stays valid inside patterns: `.status(let code, _)`.

`case let other { … }` binds the whole value and matches everything an earlier arm didn't handle. It is
allowed only when the arm uses `other`; an unused binding is a compile error (`K2304`) with a
quick fix to `default`. Like `default`, it must be the last arm, and it is a compile error when unreachable:

```k
~func example(error: NoteError) {
let message = match error {
    case .notFound { "We couldn't find that note." }
    case let other { "Something went wrong: {other}" }
}
~}
```

### Exhaustiveness

When you add a case to an enum, **every `match` that doesn't handle it becomes a compile error**, unless
it has a `default`. This is the main reason to prefer `match` over chains of `if`: the compiler tells
you every place that needs updating.

## `guard`

`guard` checks a condition and exits early when it fails:

```k
~struct Checkout {
~    let session: AppSession
~    let payments: Payments
func checkout(cart: Cart) async throws CheckoutError -> Receipt {
    guard let user = session.user else { throw CheckoutError.notLoggedIn }
    guard cart.items.isNotEmpty else { throw CheckoutError.emptyCart }

    return try await payments.charge(user, cart.total)
}
~}
```

- The `else` block **must exit**, with `return`, `throw`, `break` or `continue`. The compiler checks
  this.
- **Names bound by `guard let` stay in scope** for the rest of the function. That is the difference from
  `if let`, and why `guard` keeps the main path of a function unindented.

## `for` loops

`for … in` iterates over a collection or a range:

```k
~import k.observe.Log
~
~func reindex(products: List<String>, rows: Int, index: (String) -> Void, drawRow: (Int) -> Void) {
for product in products {
    index(product)
}

for attempt in 1...3 {                   // 1, 2 and 3
    Log.info("attempt {attempt}")
}

for row in 0..<rows {                    // 0 up to, but not including, rows
    drawRow(row)
}
~}
```

`a...b` is a closed range that includes `b`; `a..<b` is a half-open range that excludes it.

`break` leaves the loop and `continue` skips to the next element.

### Keys in views

In a view, `for` renders a node per element and **requires a `key`**, written as part of the loop:

```k
~component CartList {
~    @State var items: List<CartItem> = []
~
~    view {
for item in items key item.id {
    CartRow(item: item, quantity: bind item.quantity)
}
~    }
~}
```

The key identifies each element across updates, so the right DOM nodes move, update or disappear when
the list changes. Outside views, `key` isn't allowed.

### Streams

`for await` reads an asynchronous `Stream<T>`, such as the result of a streaming rpc, one element at a
time:

```k
~func follow(api: NotesApi, apply: (NoteChange) -> Void) async throws NoteError {
let changes = try await api.watch()

for await change in changes {
    apply(change)
}
~}
```

- The loop ends when the stream ends. A dropped connection ends it the same way; ending isn't an error.
- `break` closes the stream.
- `for await` iterates only a `Stream`, and only inside an `async` function. A plain `for` over a stream
  is a compile error (`K2310`) with a quick fix that adds `await`. It takes no `key` clause.

See [Server streaming](../apps/streaming.md).

## `while`

`while` repeats a block as long as its condition holds:

```k
~func example() {
var delay = 1.seconds

while delay < 1.minutes {
    delay = delay * 2
}
~}
```

Most loops in K code are `for` loops over collections; `while` is for the rest.
