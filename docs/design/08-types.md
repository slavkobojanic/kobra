# Types and Control Flow

## Structs and classes

There are two kinds of type with distinct jobs:

```k
// struct = value semantics. For data and state.
struct User implements Codable {
    let id: UserId
    var name: String
    var address: Address
}

// class = reference identity. For objects with a lifecycle that DI doesn't manage.
class LruCache<K, V> { ... }
```

- **Structs have value semantics.** Assigning or passing one behaves like a copy. The compiler uses copy-on-write, so the JavaScript output doesn't actually clone.
- **Nested mutation of `@State` structs is fine-grained.** `user.address.city = "Oslo"` compiles to an update along that path, and only DOM nodes that read `user.address.city` update. This makes `bind user.address.city` work without proxies or spread-everything immutability.
- **Class instances are not deeply reactive.** In `@State`, only reassignment of the reference is tracked.
- **Structs opt into derived conformances with `implements`:** `Equatable`, `Hashable`, `Codable` (JSON), `Copyable` (`user.copy(name: "Ana")`).
- Services are declared with their own keyword. See [10-services.md](10-services.md).

## Enums with associated data

```k
enum LoadState<T> {
    case idle
    case loading
    case loaded(T)
    case failed(Error)
}
```

The stdlib ships `Optional<T>` (written `T?`), `Result<T, E>` and `LoadState<T>`.

## match

`match` is exhaustive and is an expression. It works in code and in views.

```k
let label = match status {
    case .active { "On" }
    case .paused { "Off" }
}

match order.status {
    case .shipped(let tracking) where tracking.isDelayed {
        DelayBanner(tracking: tracking)
    }

    case .shipped(let tracking) {
        TrackingLink(tracking: tracking)
    }

    default {
        StatusBadge(status: order.status)
    }
}
```

- **Exhaustiveness:** adding a case to an enum causes a compile error at every `match` that doesn't handle it.
- **Guards** use `where`.
- **`default`:**
  - is **required** when matching types that can't be enumerated (`Int`, `String`, ranges such as `case 0..<10`, tuples);
  - is **allowed** on enums, as a deliberate trade-off: new cases fall into `default` silently;
  - is a **compile error when unreachable**, meaning every case is already covered.
- **`default` is the only non-binding catch-all.** A top-level `case _` arm is a compile error with a quick fix to `default`. (`_` stays valid inside patterns: `.status(let code, _)`.)
- **`case let other { … }` binds the whole value** and matches everything not handled by an earlier arm. It is allowed as a top-level arm only when the arm uses `other`; an unused binding is a compile error with a quick fix to `default`. Like `default`, it must be last, and it is a compile error when unreachable.
- `catch` follows the same rules, with `catch let error` as the binding catch-all and `catch _` as the non-binding one, because `catch` has no `default` ([09-errors.md](09-errors.md)).

## Booleans

- The logical operators are words: `and`, `or`, `not`.
- **Comparing a `Bool` with a literal is a lint** with a machine-applicable fix: `x == true` becomes `x`, and `x == false` or `x != true` becomes `not x`. `expect not screen.button(named: "Clear completed").exists` is the way to check absence; `expect`'s failure output shows the operand's value either way ([21-testing.md](21-testing.md)).

## guard

`guard` is for early exit:

```k
func checkout(cart: Cart) async throws CheckoutError -> Receipt {
    guard let user = session.user else { throw CheckoutError.notLoggedIn }
    guard cart.items.isNotEmpty else { throw CheckoutError.emptyCart }

    return try await payments.charge(user, cart.total)
}
```

- The compiler requires the `else` block to exit (`return`, `throw`, `break` or `continue`).
- Bindings from `guard let` stay in scope for the rest of the function.

## Implicit member expressions

A leading `.name` resolves against **the type expected at that spot**. This is a general rule, not something specific to colors:

```k
let c: Color = .primary                  // Color.primary
button { "Pay" }.type(.submit)           // ButtonType.submit
paragraph { ... }.class(.muted)          // this component's generated Classes type
link(to: .product(id: 42)) { "View" }    // the generated Page enum
padding: .md;                            // Spacing.md, in a style block
```

It covers enum cases, static members, theme tokens, factory methods (`.rgb(...)`) and constants.

## Extensions

Any type can be extended with methods and computed properties:

```k
extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}
```

## Lambdas

```k
let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }
    .map { product => ProductCard(product: product) }

Timer.every(30.seconds) { refresh() }       // trailing lambda with no parameters
button { "Save" }.on(click: save)            // function reference
```

- **Lambda parameters are always named:** `{ product => product.price }`. There is no implicit `it` or `$0`.
- `=>` is the lambda arrow; `->` is for return types. Typed form: `{ (product: Product) -> Money => product.price }`. Types are normally inferred.
- **Trailing lambdas** are allowed when the last parameter is a function.
- **Functions can be passed by name:** `.map(formatPrice)`.
- **A function parameter's type may carry a generic error,** `() async throws E -> T`. The call then throws whatever the passed function throws ([09-errors.md](09-errors.md#error-generic-functions)).

## Collections

```k
let names: List<String> = ["Ana", "Ben"]
let prices: Map<ProductId, Money> = [id1: 9.99, id2: 14.50]
let tags: Set<String> = Set(["new", "sale"])
```

- The core collections are `List`, `Map` and `Set`, with value semantics like structs: immutable when declared `let`, mutable when declared `var`.
- `[a, b]` is a list literal; `[key: value]` is a map literal.
- The stdlib collection API is extensive and uses full English names: `filter`, `map`, `sortedBy`, `groupedBy`, `partitioned`, `chunked`, `windowed`, `firstWhere`, `sum`, `isNotEmpty`, and so on.

## Interfaces and conformance

```k
interface PaymentProvider {
    func charge(amount: Money, card: CardToken) async throws PaymentError -> Receipt

    func supports(currency: Currency) -> Bool {      // default implementation
        return currency == .usd
    }
}

struct User implements Codable, Equatable {
    let id: UserId
    var name: String
}

service StripePayments implements PaymentProvider { ... }

func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
}
```

- **`implements` is the only conformance syntax**, with no `:` shorthand. It covers interfaces, protocols and derived conformances such as `Codable`.
- **There is no class inheritance.** Use interfaces with default methods, plus composition. This avoids fragile base classes and deep hierarchies.
- **Generic constraints use `where T implements X`,** the same `where` keyword as `match` guards.
- **Interfaces are nominal.** A type conforms only by declaring `implements`, never by matching a shape, so every implementation can be found with a single search.
- Injecting an interface is covered in [10-services.md](10-services.md).
