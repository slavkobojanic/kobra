# Types

K is statically and strongly typed. Every value has a type known at compile time, and types are erased
when K compiles to JavaScript, so they cost nothing at runtime.

## Core types

These are available everywhere without an import:

| Type | Values |
|---|---|
| `Int` | whole numbers: `42`, `-7`, `1_000_000`, `0xFF`, `0b1010`, `0o755` |
| `Double` | floating-point numbers: `3.14`, `1e9`, `2.5e-3` |
| `Decimal` | exact decimal numbers, for amounts that must not round |
| `Bool` | `true` and `false` |
| `String` | text: `"Hello, {name}"`. See [Strings](strings.md). |
| `List<T>`, `Map<K, V>`, `Set<T>` | collections. See [Collections](collections.md). |
| `Optional<T>`, written `T?` | a value or `nil` |
| `Result<T, E>` | a success value or an error |
| `LoadState<T>` | `.idle`, `.loading`, `.loaded(T)` or `.failed(Error)` |
| `Length`, `Duration`, `Percent`, `Angle` | units: `16px`, `200ms`, `50%`, `45deg`. See [Units](units.md). |
| `Date` | a point in time: `Date.now`, `2.hours.ago` |
| `Color` | `#3B82F6`, `.primary`, `.rgb(59, 130, 246)` |
| `Money`, `Currency` | an exact amount in a currency: `9.99.usd`. See [Money](units.md#money). |
| `Bytes` | an immutable byte sequence: `"text".utf8`, `Bytes.parse(hex: text)` |
| `Url` | an absolute URL: `Url("https://api.stripe.com")` |
| `Secret<T>` | a value that must not leak, such as a password or an API key |
| `Stream<T>` | an asynchronous sequence that is read once, such as a streaming rpc's result |

Other common value types live in standard-library modules and are imported: `Uuid` from `k.crypto`,
`EmailAddress` from `k.text`, `TimeOfDay` from `k.time`.

## Literals and parsing

Types whose values can be written as text follow one convention:

- **`Type("literal")` takes only a string literal**, validated at compile time, and is never optional:
  `Url("https://api.stripe.com")`, `Uuid("0b6f2c1e-…")`, `EmailAddress("orders@shop.example")`,
  `TimeOfDay("03:00")`. Passing anything else is a compile error, with a quick fix to `.parse`.
- **`Type.parse(_ text: String) -> Type?` is the only runtime parse.** When a type has several text
  forms, the label names the form: `Bytes.parse(hex:)`, `Bytes.parse(base64:)`, `Currency.parse(code:)`.

```k
let api = Url("https://api.open-meteo.com")             // checked when you compile
let callback = Url.parse(request.query["next"] ?? "")   // Url?, checked when it runs
```

There are no failable initializers, so the same spelling never means "checked literal" for one type and
"may fail" for another.

## Booleans and logic

Logical operators are words:

```k
let canCheckout = isSignedIn and cart.items.isNotEmpty
let showBanner = isOffline or hasError
let isGuest = not isSignedIn
```

`&&`, `||` and `!` are compile errors with a quick fix that replaces them.

**Don't compare a `Bool` with a literal.** `x == true` is just `x`, and `x == false` is `not x`. The
compiler reports the comparison with a machine-applicable fix:

```k
let isPinned = not note.isPinned       // not `note.isPinned == false`
expect not screen.alert().exists       // not `screen.alert().exists == false`
```

## Optionals

A type followed by `?` may hold a value or `nil`:

```k
var nickname: String? = nil
nickname = "Ana"
```

You can't use an optional as if it were a value. Unwrap it first, with one of these:

```k
// if let: run a block only when there is a value
if let nickname {
    Log.info("greeting {nickname}")
}

// guard let: exit early when there isn't
guard let user = session.user else { return }

// ?. : optional chaining; the whole expression is nil if any step is
let city = user?.address.city

// ?? : a default value
let displayName = nickname ?? "friend"
```

There is no force-unwrap operator. K has no `!`, and no way to pretend an optional is present.

## Type inference

Types are inferred from initial values and from context. You write types on function signatures and
public fields, and wherever inference has nothing to go on:

```k
let count = 3                            // Int
let names = ["Ana", "Ben"]               // List<String>
let empty: List<String> = []             // the element type must be given
```

## Implicit member expressions

A leading `.name` resolves against **the type expected at that spot**. This one rule is behind much of
K's readable syntax:

```k
let c: Color = .primary                  // Color.primary
button { "Pay" }.type(.submit)           // ButtonType.submit
paragraph { "…" }.class(.muted)          // this component's generated Classes type
link(to: .product(id: 42)) { "View" }    // the generated Page enum
padding: .md;                            // Spacing.md, in a style block
```

It works for enum cases, static members, theme tokens, factory methods such as `.rgb(…)`, and
constants. When there is no expected type, there is nothing to resolve against, and you write the type
name: `Color.primary`.

## Equality and comparison

`==` and `!=` compare values. Structs and enums compare by value when they implement `Equatable`, which
the compiler can derive for you. `<`, `<=`, `>` and `>=` compare numbers, units, dates and other ordered
types.

Comparisons don't chain: `a < b < c` is a compile error. Write `a < b and b < c`.

## Tuples

A tuple groups a fixed number of values of possibly different types:

```k
let bounds: (Int, Int) = (0, 100)
```

Tuples are taken apart with patterns, in `for` and `match`, not by position: there is no `bounds.0`. For
anything more than a quick pair, declare a [struct](declarations.md#structs) with named fields.

## Declaring your own types

K has a small set of kinds of type, each with a distinct job:

| Kind | Use it for |
|---|---|
| `struct` | data with value semantics: state, requests, responses |
| `enum` | a value that is one of several cases, each with optional associated data |
| `class` | objects with identity and a lifecycle that dependency injection doesn't manage |
| `interface` | a set of requirements that other types implement |

[Structs, enums, classes and interfaces](declarations.md) covers them. Application-level kinds, such as
`component`, `service` and `entity`, have their own chapters.
