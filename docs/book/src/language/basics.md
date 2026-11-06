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
var visits = 0

func welcome(name: String) -> String {
    visits += 1

    return "{greeting}, {name}! You are visitor number {visits}."
}
```

Even this small snippet shows several of K's rules:

- Every declaration states its mutability: `let` or `var`.
- Types are inferred where they're obvious, and written where they're part of an API (`name: String`,
  `-> String`).
- Strings interpolate with `{ }`.
- **There are no semicolons.** A newline ends a statement, and there is exactly one statement per line.

## Comments

```k
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
annotations**, so `/// …` goes above `@Prop` or `@Param`. The body is Markdown. The first sentence is
the summary shown in autocomplete and in `k doc`; further paragraphs follow a blank `///` line.
Structured sections use Markdown list items, as in Swift:

```k
/// Looks up a single place by its id.
///
/// - Parameter id: the id from a `Place`, usually taken from the URL.
/// - Returns: the place, from the cache when it has been seen before.
/// - Throws: `WeatherError.placeNotFound` when no place has this id.
public func place(id: Int) async throws WeatherError -> Place { … }
```

- Names in backticks that resolve to declarations become links, and the compiler checks them: a
  reference to something that doesn't exist is a warning, so documentation can't drift after a rename.
- **What counts as a reference:** a backticked identifier, a dotted path of identifiers
  (`` `WeatherError.offline` ``), or a function name with its labels (`` `searchPlaces(query:)` ``).
  Other code spans, keywords included (`` `300ms` ``, `` `"todos"` ``, `` `nil` ``, `` `a + b` ``), are
  plain code and aren't checked.
- **A reference resolves against three things only:** the file's imports, the file's own package (an
  application counts as one package), and fully qualified names (`` `k.http.HttpStatus` ``). A reference
  that resolves against none of these is a warning, even when some other module declares the name.
- Fenced code blocks tagged `k` inside a doc comment are type-checked by `k check`, but not run.
- There are no block doc comments (`/** */`), and no Javadoc-style `@param`, because `@` already means an
  annotation.

**What must be documented** is what is API: public top-level declarations, their non-private functions,
computed properties and enum cases, and the annotated inputs of components and pages (`@Prop`,
`@Binding`, `@Slot`, `@Emits`, `@Stylable`, `@Param`). Stored fields of public structs, entities and
configs are a lint only, and injected fields never need documentation. In a package, `k check` warns on
missing documentation; in an application it is a lint.

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
let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }

let isReady = hasLoaded
    and isSignedIn
```

`-` is the one exception: a line that starts with `-` begins a new statement with a unary minus. Break a
long subtraction after the operator instead.
