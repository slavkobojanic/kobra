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
- **Every struct has a memberwise initializer** with one named argument per stored field, in declaration order: `User(id: id, name: "Ana", address: home)`. **A field with a default value may be omitted** and takes its default; **omitting a field without one is a compile error**, with a quick fix that adds the missing arguments. Entities are constructed the same way ([19-persistence.md](19-persistence.md#entities-and-tables)).
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

- **A payload field may have a default,** as a function parameter may: `case posted(room: String, number: Int, edited: Bool = false)`. Building the case may leave the field out (`.posted(room: "help", number: 3)`), and decoding a message that leaves it out (one from a sender built before the field was added) gives the default, as for a struct's field. So adding a field with a default to a case that a protocol carries is compatible in `rpc.lock`, like adding an optional field ([44-channels.md](44-channels.md#rpclock-and-compatibility)). The default is checked against the field's type, and runs where the case is built or decoded, in the browser and on the server alike.
- **A pattern still names every field** (`case .posted(let room, _, _)`): a pattern that could leave out defaulted fields would match differently once one is added, and the added field's `_` is a one-line edit the compiler points to. Why defaults rather than only optionals: an optional field makes every reader handle `nil` for a value that always has an obvious one (`edited: false`). **Alternatives:** optional fields only (the old rule; `nil` checks everywhere); a new case for the new shape (two cases for one event, and every `match` gains an arm).

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

clock.every(30.seconds) { refresh() }       // trailing lambda with no parameters
button { "Save" }.on(click: save)            // function reference
```

- **Lambda parameters are always named:** `{ product => product.price }`. There is no implicit `it` or `$0`.
- `=>` is the lambda arrow; `->` is for return types. Typed form: `{ (product: Product) -> Money => product.price }`. Types are normally inferred.
- **The last expression of a lambda's body is its value.** `return` inside a lambda is a compile error, so `return` only ever leaves a declared function:

  ```k
  let marked = await database.transaction {
      let marked = await orders.markPaid(id: orderId, reference: reference)

      if marked {
          await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: orderId))
      }

      marked                                  // the lambda's value; `return marked` is a compile error
  }
  ```

  Other blocks used as values (an `if` or `match` arm used as an expression, a computed property) also take their last expression as the value. Functions with a return type use `return`.
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

## Generics

```k
let note = try Json.decode<Note>(text)            // explicit type arguments
let vault = KeyName<AesGcmKey>(name: "vault")
let animals: List<Animal> = dogs                  // read-only positions are covariant
```

- **Type arguments are inferred** from the arguments and the expected type, **or written after the name** when inference has nothing to go on: a function (`decode<User>(json)`), a member's own generics (`Jwt.verify<Claims>(…)`, `names.map<Int> { … }`), a generic type's initializer (`KeyName<AesGcmKey>(name: "vault")`), a static member of a generic type (`Sealed<Note>.parse(text)`) and a generic component (`Picker<Int>(…)`).
- **Read-only positions are covariant:** a `List<Dog>` is a `List<Animal>` when `Dog` implements `Animal`. Mutable positions (`var` bindings and fields) and a project's own generic types are invariant. There are no variance annotations.
- **Protocols and rpcs can't be generic**, since `rpc.lock` records each rpc's concrete types.

## Decided while implementing

**Explicit type arguments**

- **The syntax is `name<Type, …>` directly after a name or member name,** followed by `(`, `.` or a trailing lambda: `decode<User>(json)`, `RemoteClient.at<PaymentEvents>(…)`, `Sealed<Note>.parse(text)`, `names.map<Int> { … }`. The parser takes the `<` as type arguments only when the list is written as types are, with no space after the name or inside the brackets' ends, uppercase type names and on one line ([grammar RA-23](../compiler/grammar.md#ra-23-explicit-generic-arguments-in-expressions)). Comparisons don't chain (K0113), so no valid comparison changes meaning; the remaining ambiguity, `f(a < b, c > (d))`, stays two arguments because comparisons are spaced. Rejected: a turbofish (`decode::<User>`), which reads as noise and is unlike the type syntax everywhere else, and C#'s rule of looking only at the token after `>`, which turns `f(a<b, c>(d))` into a generic call.
- **`RemoteClient.at<P>(…)`, not `RemoteClient<P>.at(…)`.** `P` is the generic parameter of the static function `at`, not of `RemoteClient`, which isn't generic; `RemoteClient<P>.at` is K2020. Making `RemoteClient` generic to support the other spelling would give every other use of the type a meaningless parameter.
- **Explicit arguments win over the expected type.** `let k: KeyName<HmacKey> = KeyName<AesGcmKey>(name: "k")` is K2001 at the initializer, not a silent re-inference.
- **One type per generic parameter (K2020),** in expressions and in types alike: `decode<Int, String>(…)` and `Box<Int, Int>` are errors at the `<…>` list (before this round, extra type arguments in a type were dropped silently). Type arguments on something that isn't generic (a local, a field of function type, a non-generic function) are K2020 with a fix that removes them. After K2020 the generics are `{error}`, so nothing that depends on them is reported again. There is no partial list (`pair<Int, _>`): Kobra has no type placeholder, and a partial list rarely reads better than an annotated binding.
- **Constraints apply to written arguments as to inferred ones** (K2009 at the type argument).
- **Generics cost nothing at run time:** explicit arguments only fix what inference would otherwise find, so the emitted JavaScript is the same, codecs and witnesses included.
- **Not supported:** a generic function value with its arguments (`list.map(decode<User>)`, which the parser doesn't take as type arguments since no `(` follows); give the function type by the expected type instead. Higher-kinded types stay out of scope.
- **The tooling shows them.** Signature help in `decode<User>(` labels the signature `decode<User>(_ text: String) -> User`, with the type parameters replaced by what is written (a type's initializers and memberwise fields too), and hover on the name adds "Called as `decode<User>(…) -> User`".
- **The examples use them where they read better:** keys declared as `let savedTodos = StorageKey<List<Todo>>(name: "todos")` and `let vaultKey = KeyName<AesGcmKey>(name: "notes-vault")`, and `RemoteClient.at<PaymentEvents>(…)`. Bindings whose annotation already names the type (`let event: StripeEvent = try Json.decode(payload)`) stay as they are.

**Variance**

- **Covariant positions are fixed by the standard library, not declared:** the elements of `List`, `Set` and `Stream`, a `Map`'s values (keys stay invariant, since a key is hashed and compared as its own type), both arguments of `Result`, an optional's value and a function's result (its parameters are contravariant, as before). A project's own generic types, classes included, are invariant; variance annotations stay out of scope.
- **`var`s are invariant.** A `var` local's initializer, an assignment, a `var` field's initializer and a `var` field's memberwise argument must have exactly the declared type inside, so `var animals: List<Animal> = dogs` is K2001 with a help, while `let` takes it. Kobra's collections are values, so this isn't needed for soundness today; it keeps a mutable binding's type the one written, and it means turning a collection-holding field into a class (with reference identity) never makes code that compiled unsound.
- **A conversion that changes how a value is held is made only where codegen makes it.** A struct, enum or entity held as a project interface is wrapped in its adapter, a concrete error as `Error` is boxed with its text, a value as `Codable` carries its codec. Those conversions apply to a value itself, an optional's value and a list's elements (also of an optional list, `List<Card>` as `List<Priced>?`), each converted by the existing `coerce` (a list is mapped once). Inside a `Set`, a map's values, a `Result`, or a function's parameters and result, the conversion would need a copy of the container (or a wrapper function) on every use, so it is K2001 with a help that says to convert first. Classes, services and the stdlib's other interfaces keep the value as it is, so they convert in every covariant position. Rejected: allowing all of them and copying containers in codegen, which would hide an O(n) copy behind an assignment and grow the runtime.
- **Function references no longer convert their result to an interface.** Before this round a `() -> Card` was accepted as a `() -> Priced` and called without the adapter; now it is K2001 with a help suggesting a lambda (`{ make() }`), which is converted. A function whose result is a class still converts.

**Generic protocols and rpcs**

- **A protocol or an rpc with generic parameters is K5514** at the parameter list, in a protocol or in a service. `rpc.lock` records each rpc's concrete parameter, result and error types, and a deployed caller built with one type can't call a handler built with another. The parser now accepts the list (and an rpc's `where` clause) so the diagnostic can say why; a generic protocol's rpcs aren't checked further, and a generic handler isn't compared with its rpc (no K5505 on top).

**Generic components and services**

- **A generic component's type arguments are inferred from its props per placement, or written out** (`Picker<Int>(title: …)`). `crates/k_cli/tests/fixtures/generics` places one `Picker<T>` both ways, with `k test` and in real Chrome (`chrome.rs`, `a_generic_component_renders_and_emits_its_type_in_chrome`).
- **An injected service's and an interface's generic members** take explicit arguments like any other call (`basket.last<Fruit>()`, decoding JSON as the type asked for, and `ranker.best<Fruit>(…)` through the interface `Ranker` bound to its one service). Services themselves stay non-generic: a service is a single instance per scope, so a type parameter would have nothing to vary over.

**Small changes made along the way**

- **`try … else { }` where an optional is expected widens the operand to its optional,** so the fallback may be `nil`: `let value: T? = try Json.decode<T>(text) else { nil }`. Before, the `else` block had to produce the operand's own type, which a written-out `T` made impossible to express.
