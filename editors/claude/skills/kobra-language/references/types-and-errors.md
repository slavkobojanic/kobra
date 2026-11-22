# Types, declarations and errors

Sources: `book/language/types.md`, `declarations.md` and `errors.md`, and `design/08-types.md` and
`09-errors.md`.

## Core types (no import)

`Int`, `Double`, `Decimal`, `Bool`, `String`, `List<T>`, `Map<K, V>`, `Set<T>`, `T?`,
`Result<T, E>`, `LoadState<T>` (`.idle`, `.loading`, `.loaded(T)`, `.failed(Error)`), `Length`,
`Duration`, `Percent`, `Angle`, `Date`, `Color`, `Money`, `Currency`, `Bytes`, `Url`, `Secret<T>` and
`Stream<T>`. Imported value types include `k.crypto.Uuid`, `k.text.EmailAddress` and
`k.time.TimeOfDay`.

**Literals and parsing.** `Type("literal")` takes only a string literal, checked at compile time, and
is never optional: `Url("https://api.stripe.com")`, `Uuid("0b6f…")`, `EmailAddress("a@b.example")`,
`TimeOfDay("03:00")`. An interpolated argument is K2403, and an invalid literal is K2404. The only
runtime parse is `Type.parse(text)`, which returns `Type?`. A `Url` is absolute (`http`, `https`, `ws`
or `wss`), so `Url("/icon.svg")` doesn't compile.

**Implicit members.** A leading `.name` resolves against the expected type: `let c: Color = .primary`,
`.type(.submit)`, `link(to: .product(id: 42))` and `padding: .md`. With no expected type, write the
type: `Color.primary`.

**Variance.** A `List<Dog>` is accepted as a `List<Animal>` where it can't change, but a `var` or a
`Map` key is invariant (K2001).

## Structs

```k
struct Address implements Codable, Equatable {
    var street: String
    var city: String
    var country: Country = .norway
}

struct User implements Codable, Equatable, Copyable {
    let id: Uuid
    var name: String
    var address: Address

    var isNorwegian: Bool { address.country == .norway }

    func greeting() -> String {
        return "Hello, {name}"
    }
}
```

- You get a memberwise initializer, and defaulted fields become optional arguments:
  `Address(street: "Storgata 1", city: "Oslo")`. An initializer can't leave out fields that have no
  default.
- Structs are **values**: `var draft = user` and then `draft.name = "Ana"` leaves `user` unchanged.
  A `let` struct is fully immutable.
- The derived conformances are `Equatable` (`==`), `Hashable` (a map key or set element), `Codable`
  (JSON and RPC) and `Copyable` (`user.copy(name: "Ana Maria")`).
- A custom `init` sets every field:

```k
struct Temperature {
    let celsius: Double

    init(fahrenheit: Double) {
        celsius = (fahrenheit - 32) * 5 / 9
    }
}
```

- `static let` and `static func` give type-level members, used with implicit member syntax:

```k
struct Padding {
    let size: Length

    static let compact = Padding(size: 4px)
    static let comfortable = Padding(size: 12px)
}
```

- `@Builder` makes required fields arguments and defaulted fields chained calls:

```k
@Builder
struct Toast {
    let message: String
    let duration: Duration = 4.seconds
    let tone: Tone = .neutral
}
```

It's used as `Toast(message: "Saved").duration(2.seconds).tone(.success)`.

## Enums

Each case goes on its own line. Cases may carry data, and payload fields may have defaults.

```k
enum Shape {
    case circle(radius: Length)
    case rectangle(width: Length, height: Length)
    case point
}
```

```k
enum Plan {
    case free
    case pro
    case team

    var seats: Int {
        match self {
            case .free, .pro { 1 }
            case .team { 10 }
        }
    }
}
```

`Optional`, `Result` and `LoadState` are ordinary generic enums. Adding a case makes every `match`
that doesn't handle it an error (K2301), unless it has a `default`.

## Interfaces and classes

```k
interface Greeter {
    func name() -> String

    func greet() -> String {
        return "Hello, {name()}"
    }
}
```

- Interfaces are **nominal**: a type conforms only by writing `implements`. They may give default
  implementations, and may require fields (`let price: Money`).
  `interface Searchable implements Identifiable` makes every implementation implement both.
- `interface` is an in-process abstraction. `protocol` means a **wire contract** (RPC), which is
  something else (see the kobra-apps skill).
- A `class` has reference identity, and is for objects with a lifecycle that dependency injection
  doesn't manage. **There's no inheritance**, and only reassigning a class reference in `@State` is
  tracked.

```k
class LruCache<K, V> where K implements Hashable {
    private var entries: Map<K, V> = [:]
    let capacity: Int

    init(capacity: Int) {
        self.capacity = capacity
    }

    func get(key: K) -> V? {
        return entries.get(key)
    }

    func set(key: K, value: V) {
        entries.set(key, value)
    }
}
```

## Generics and extensions

```k
func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
}

struct RetryPolicy {
    let attempts: Int
}

extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}

extend<T> List<T> where T implements Priced {
    var totalPrice: Money { self.sum(in: .usd) { item => item.price } }
}
```

`3.retries` and `products.totalPrice` then work. A `public extend` is visible wherever its type is,
and a plain `extend` only within its module.

## Errors

An error type is an enum that `implements Error`. `throws` names exactly one type, and there's no
untyped `throws` (K0129) and no `throws A | B`.

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

struct Stock {
    let available: Int
}

struct Reservation {
    let quantity: Int
}

func reserve(stock: Stock, quantity: Int) throws CartError -> Reservation {
    guard stock.available >= quantity else {
        throw CartError.outOfStock(available: stock.available)
    }

    return Reservation(quantity: quantity)
}
```

### Handling: `do`/`catch` is exhaustive

```k
func stockMessage(stock: Stock) -> String {
    do {
        let reservation = try reserve(stock: stock, quantity: 2)
        return "Reserved {reservation.quantity}"
    } catch .outOfStock(let available) where available == 0 {
        return "Sold out"
    } catch .outOfStock(let available) {
        return "Only {available} left"
    } catch .productNotFound, .unauthorized {
        return "Unavailable"
    }
}
```

- Catch clauses use `match` patterns. A case written without its payload matches any payload
  (`catch .rpcFailure`).
- `catch let error` handles the rest, and `error` has the `do` block's static error type. If the
  binding goes unused that's K2304, so write `catch _`.
- A catch-all that can't be reached is an error, and `catch` has no `default`.
- `catch unknown` comes after clauses that name every case. It runs only for cases added later, and
  the compiler warns (K2207) when that happens:

```k
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

### Wrapping at layer boundaries (errors are wrapped, not unioned)

```k
enum CheckoutError implements Error {
    case emptyCart
    case cart(CartError)
}

func checkout(stock: Stock, quantity: Int) throws CheckoutError -> Reservation {
    guard quantity > 0 else { throw CheckoutError.emptyCart }

    do {
        return try reserve(stock: stock, quantity: quantity)
    } catch let error {
        throw CheckoutError.cart(error)
    }
}
```

### Errors as values

```k
func reserveOrNothing(stock: Stock) -> Reservation? {
    let reservation = try reserve(stock: stock, quantity: 1) else { return nil }
    return reservation
}

func outcome(stock: Stock) -> Result<Reservation, CartError> {
    return Result.of { try reserve(stock: stock, quantity: 1) }
}
```

### Rules worth remembering

- A missing `try` is K2201. A non-exhaustive `catch` is K2206.
- **Event handlers and `@Effect`s can't throw** (K3017 and K3012). Handle the error inside, usually
  by storing it in `@State`.
- Error-generic functions pass errors through: `func transaction<T, E>(_ block: () async throws E
  -> T) async throws E -> T`. A block that can't throw makes the call need no `try`.
- **Panics aren't errors.** An index out of range, a failed `assert`, a lost database connection: none
  of them can be caught. An `ErrorBoundary` catches panics in a view.
- Every rpc's error enum has the fixed case `rpcFailure(RpcFailure)`, or the rpc uses `RpcOnlyError`
  (see the kobra-apps skill).
- Test errors with `expect throws CartError.outOfStock { try … }`.
