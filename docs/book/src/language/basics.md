# The basics

This part of the book covers K as a general-purpose language: the values, functions and types you use
in every kind of declaration, whether it's a component, a service or a test.

If you know Swift, you'll recognize a lot. If you know TypeScript or Kotlin, the ideas will be
familiar even where the spelling differs. The chapters are:

- [Constants and variables](constants-and-variables.md): `let`, `var` and type inference.
- [Functions](functions.md): `func`, parameters, labels, defaults and effects.
- [Types](types.md): the core types, optionals and implicit member expressions.
- [Strings and interpolation](strings.md).
- [Units and number extensions](units.md): `16px`, `200ms`, `10.minutes`.
- [Collections](collections.md): `List`, `Map` and `Set`.
- [Lambdas](lambdas.md): `{ product => product.price }`.

## A first look

```k
let greeting = "Hello"

func welcome(name: String, visitsSoFar: Int) -> String {
    var visits = visitsSoFar
    visits += 1

    return "{greeting}, {name}! You are visitor number {visits}."
}
```

Even this small snippet shows several of K's rules:

- Every declaration states its mutability: `let` or `var`. Top-level values are always `let`; mutable
  state lives in a function, a component or a service.
- Types are inferred where they're obvious, and written where they're part of an API (`name: String`,
  `-> String`).
- Strings interpolate with `{ }`.
- **There are no semicolons.** A newline ends a statement, and there is exactly one statement per line.

## Comments

```k
~struct CartItem {
~    let price: Money
~    let quantity: Int
~}
~
// An ordinary comment. Tools ignore it.

/* A block comment,
   which may span lines. */

/// A documentation comment. It documents the declaration that follows it.
/// The body is Markdown.
public func total(items: List<CartItem>) -> Money {
    return items.sum(in: .usd) { item => item.price * item.quantity }
}
```

Documentation comments start with `///` and come before the declaration **and before its
annotations**, so `/// …` goes above `@Prop` or `@Param`. The body is Markdown: the first sentence is the
summary shown in completion and on hover, and structured sections are Markdown list items, as in Swift.
A code span in square brackets, such as `` [`WeatherError.placeNotFound`] ``, is a link to a
declaration, and the compiler checks that it resolves.
[Documentation comments](doc-comments.md) covers the rules.

## Naming

- Types, components, pages and other declarations of a kind are `UpperCamelCase`: `CartService`,
  `ProductCard`.
- Functions, fields, parameters and enum cases are `lowerCamelCase`: `addToCart`, `isExpanded`,
  `.outOfStock`.
- Booleans read as questions: `isOpen`, `isEmpty`, `hasCompleted`.
- **Names are spelled out.** `description`, not `desc`; `button`, not `btn`. Well-established terms such
  as `id`, `url` and `rpc` are fine.
- Identifiers are ASCII. Strings and comments may contain any Unicode text.

## Line breaks

Because newlines end statements, a long expression continues on the next line only when the next line
starts with something that can't begin a new statement, such as a `.` or a binary operator:

```k
~struct Product {
~    let stock: Int
~    let price: Money
~}
~
~let products: List<Product> = []
~let hasLoaded = true
~let isSignedIn = false
~
let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }

let isReady = hasLoaded
    and isSignedIn
```

`-` is the one exception: a line that starts with `-` begins a new statement with a unary minus. Break a
long subtraction after the operator instead.
