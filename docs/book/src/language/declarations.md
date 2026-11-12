# Structs, enums, classes and interfaces

K has four general-purpose kinds of type. Each has one job:

| Kind | Semantics | Use it for |
|---|---|---|
| `struct` | value | data and state |
| `enum` | value | one of several cases, each with optional data |
| `class` | reference | objects with identity and a lifecycle that dependency injection doesn't manage |
| `interface` | — | requirements that other types implement |

Services, components and entities have their own keywords and chapters. This chapter covers the four
building blocks.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/people/Context.k
enum Country implements Equatable, Codable {
    case norway
    case sweden
}

struct UserId implements Equatable, Codable {
    let value: Int
}

enum OrderStatus implements Codable {
    case pending
    case paid
}

enum Tone {
    case neutral
    case success
}

struct CardToken {
    let value: String
}

enum PaymentError implements Error {
    case declined
}

struct Receipt {
    let amount: Money
}

struct RetryPolicy {
    let attempts: Int
}

interface Priced {
    let price: Money
}
```
-->

## Structs

A struct groups named fields:

```k
struct Address implements Codable, Equatable {
    var street: String
    var city: String
    var country: Country = .norway
}

struct User implements Codable, Equatable {
    let id: UserId
    var name: String
    var address: Address
}
```

A struct gets a **memberwise initializer**. Fields with a default value become optional arguments:

```k
~struct Address {
~    var street: String
~    var city: String
~    var country: Country = .norway
~}
~
~struct User {
~    let id: UserId
~    var name: String
~    var address: Address
~}
~
~func example(id: UserId) {
let home = Address(street: "Storgata 1", city: "Oslo")
let user = User(id: id, name: "Ana", address: home)
~}
```

**Members follow their type.** The fields, functions, computed properties, static members and `init` of a
`public` struct are public, with no modifier; those of a module-private struct are visible within its
module. `private` is the only member modifier, and it hides a member from everything outside the
declaration's body:

```k
~import k.text.EmailAddress
~import k.crypto.Uuid
public struct Order implements Codable {
    let id: Uuid
    let email: EmailAddress
    var status: OrderStatus

    private let internalNote: String = ""       // only Order's own body can read this
}
```

Writing `public` on a member is redundant. It is a warning (`K1302`) with a machine-applicable fix, which
`k fmt` applies. See [Modules and visibility](modules.md#visibility-of-members).

### Value semantics

**Structs behave like values.** Assigning or passing a struct behaves like a copy, so changing one copy
never affects another:

```k
~struct User {
~    var name: String
~}
~
~func example(user: User) {
var draft = user
draft.name = "Ana Maria"         // user.name is still "Ana"
~}
```

`let` makes a struct value fully immutable, including its `var` fields. The compiler uses copy-on-write,
so the JavaScript output doesn't actually clone anything until a copy changes.

### Fine-grained updates in state

Value semantics are what make nested reactive state simple. Mutating a nested field of a `@State` struct
updates along that exact path:

```k
~struct Address {
~    var city: String
~}
~
~struct User {
~    var address: Address
~
~    static let sample = User(address: Address(city: "Oslo"))
~}
~
~component Profile {
@State var user: User = .sample

func move() {
    user.address.city = "Bergen"      // only DOM nodes that read user.address.city update
}
~    view {
~        text { user.address.city }
~    }
~}
```

This is also what makes `bind user.address.city` work, with no proxies and no spread-everything
immutability.

### Derived conformances

Structs opt into common behavior with `implements`. The compiler generates the implementation:

| Conformance | Gives you |
|---|---|
| `Equatable` | `==` and `!=`, comparing every field |
| `Hashable` | use as a `Map` key or `Set` element |
| `Codable` | JSON encoding and decoding, and use in RPC messages |
| `Copyable` | `user.copy(name: "Ana")`, a copy with some fields changed |

### Initializers

When a struct needs more than its memberwise initializer, it declares an `init`:

```k
struct Temperature {
    let celsius: Double

    init(fahrenheit: Double) {
        celsius = (fahrenheit - 32) * 5 / 9
    }
}
```

### Builder types

Annotate a struct with `@Builder` and it follows the same rule as elements and components: **required
fields are named arguments, fields with defaults are chained.** The compiler generates the chained
methods.

```k
@Builder
struct Toast {
    let message: String
    let duration: Duration = 4.seconds
    let tone: Tone = .neutral
}

let saved = Toast(message: "Saved")
    .duration(2.seconds)
    .tone(.success)
```

## Enums

An enum is a value that is exactly one of its cases. **Each case is declared on its own line:**

```k
enum Status {
    case active
    case paused
}
```

### Associated data

Cases can carry data, which makes enums K's sum types:

```k
enum Shape {
    case circle(radius: Length)
    case rectangle(width: Length, height: Length)
    case point
}

let shape: Shape = .circle(radius: 4px)
```

Enums can be generic. The standard library's `LoadState` is an ordinary enum:

```k
enum LoadState<T> {
    case idle
    case loading
    case loaded(T)
    case failed(Error)
}
```

`Optional<T>` (written `T?`) and `Result<T, E>` are enums too. You take enums apart with
[`match`](control-flow.md#match), which is exhaustive, so adding a case shows you every place that must
handle it.

A payload field can have a default, as a function parameter can. Building the case may leave it out:

```k
enum RoomEvent implements Codable {
    case posted(room: String, number: Int, edited: Bool = false)
}

let event: RoomEvent = .posted(room: "help", number: 3) // edited: false
```

Decoding a message without the field gives the default too, so adding a field with a default to a case
a protocol carries doesn't break tabs or callers already deployed: `rpc.lock` accepts it as it accepts an
optional field ([`rpc.lock`](../apps/workspaces.md#rpclock-deploys-that-dont-break-callers)). A pattern still names every field,
`case .posted(let room, _, _)`.

### Methods on enums

Enums can have methods and computed properties:

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

### Error enums

An enum that `implements Error` can be thrown. This is how all errors are declared in K; see
[Errors](errors.md):

```k
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}
```

## Classes

A class has **reference identity**: two names can refer to the same instance, and a change through one is
visible through the other.

```k
class LruCache<K, V> where K implements Hashable {
    private var entries: Map<K, V> = [:]
    let capacity: Int

    init(capacity: Int) {
        self.capacity = capacity
    }

    func get(key: K) -> V? {
        // …
~        return entries.get(key)
    }

    func set(key: K, value: V) {
        // …
~        entries.set(key, value)
    }
}
```

Use a class for objects with a lifecycle that dependency injection doesn't manage, such as a cache owned
by one service. Data and state belong in structs; shared app logic belongs in
[services](../apps/services.md).

- **Class instances are not deeply reactive.** In `@State`, only reassigning the reference is tracked.
- **There is no class inheritance.** Classes implement interfaces and compose other objects instead. This
  avoids fragile base classes and deep hierarchies.

## Interfaces

An interface lists requirements. Types conform to it **by declaring `implements`**:

```k
interface PaymentProvider {
    func charge(amount: Money, card: CardToken) async throws PaymentError -> Receipt

    func supports(currency: Currency) -> Bool {      // a default implementation
        return currency == .usd
    }
}

service StripePayments implements PaymentProvider {
    func charge(amount: Money, card: CardToken) async throws PaymentError -> Receipt {
        // …
~        throw .declined
    }
}
```

- **`implements` is the only conformance syntax.** It covers interfaces, protocols and derived
  conformances such as `Codable`. There is no `:` shorthand.
- **Interfaces can provide default implementations.** Together with composition, they replace
  inheritance.
- **Interfaces are nominal.** A type conforms only by declaring `implements`, never by happening to have
  the right shape, so every implementation can be found with one search.
- `interface Searchable implements Identifiable` means every implementation of `Searchable` must also
  implement `Identifiable`.

K's `interface` is for in-process abstractions. A `protocol` is something else entirely: a wire contract
between browser and server. See [Protocols and RPC](../apps/protocols.md).

## Generics

Structs, enums, classes, interfaces and functions can take type parameters. Constraints are written in a
`where` clause:

```k
struct Batch<T> where T implements Codable {
    let items: List<T>
    let nextCursor: String?
}

func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
}
```

The same `where` keyword introduces guards in `match`, so constraints read as prose: "where T
implements Priced".

## Extensions

Any type can be extended with methods and computed properties, including types from the standard
library:

```k
extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}

extend Date {
    var isMidnightUtc: Bool { unixSeconds % 86400 == 0 }
}
```

Generic types are extended with their type parameters after `extend`:

```k
extend<T> List<T> where T implements Priced {
    var totalPrice: Money { self.sum(in: .usd) { item => item.price } }
}
```

Extensions are how the number extensions in [Units](units.md) work, and how tests add fixtures such as
`Product.sample`.
