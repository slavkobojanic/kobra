# Kobra syntax cheat-sheet

Sources: `book/language/basics.md`, `constants-and-variables.md`, `functions.md`, `control-flow.md`,
`strings.md`, `lambdas.md`, `collections.md`, `units.md`, `book/reference/operators.md` and
`keywords.md`. The grammar, `compiler/grammar.md`, is authoritative.

## Statements and lines

- One statement per line, with no `;`. A long expression continues on a line that starts with `.` or
  a binary operator. Break a subtraction *after* the `-`.
- Comments are `//`, `/* */` and `///` doc comments. Doc comments are Markdown and go **above the
  annotations** of the declaration they document. Link a declaration with `` [`Name`] ``, which the
  compiler checks (K1901).
- Identifiers are ASCII. Types and declarations are `UpperCamelCase`, members and enum cases are
  `lowerCamelCase`, and booleans read as questions (`isOpen`). Names are spelled out (`description`,
  not `desc`).

## `let`, `var`, top level

```k
let defaultPageSize = 50

func clamp(value: Int, low: Int, high: Int) -> Int {
    if value < low {
        return low
    }
    if value > high {
        return high
    }
    return value
}
```

- `let` is a constant and `var` a variable. Prefer `let`. Assigning a `let` is K2011.
- A top-level `var` is an error (K0131).
- Write the type when there's no initializer or it isn't inferable: `let names: List<String> = []`,
  `var selected: Product? = nil`.

## Functions

```k
func formatted(amount: Decimal, decimals: Int = 2, grouping: Bool = true) -> String {
    return "{amount}"
}
```

- Arguments are labelled with the parameter name (`formatted(amount: price, decimals: 0)`). `_`
  makes an argument positional (`func find(_ id: Id)` is called as `find(id)`). A separate external
  label is written first: `func reviews(for product: ProductId)` is called as `reviews(for: id)`.
- Effects come in a fixed order: `func load(id: Id) async throws LoadError -> Product`. Call it as
  `try await load(id: id)`. `try` and `await` start the expression they cover.
- Generic functions use `where` constraints with `implements`:
  `func cheapest<T>(items: List<T>) -> T? where T implements Priced`. Explicit type arguments follow
  the name with no space (`Json.decode<Draft>(text)`, `LruCache<String, Int>(capacity: 10)`).
- Function types: `(Int) -> Int`, `(ProductId) async throws CatalogError -> Product`, `() -> Void`.
- A computed property is a `var` with a block: `var isSquare: Bool { width == height }`.
- Plain functions use default parameters. Elements, components, pages and `@Builder` structs use
  *chained calls* for optional values instead.

## Control flow

`if`, `match`, `guard`, `for` and `while` work the same in code and in views. Conditions take no
parentheses.

```k
func sizeLabel(count: Int) -> String {
    let size = match count {
        case 0 { "empty" }
        case 1..<10 { "small" }
        default { "large" }
    }
    return size
}
```

- **`if` is an expression** (there's no ternary). Used as a value it needs an `else`, and each
  branch's last expression is its value.
- **`if let`** unwraps an optional: `if let user = session.user { … }`, or just `if let nickname { … }`.
  Several conditions are separated by commas:
  `if let user = session.user, let address = user.address, address.country == .norway { … }`.
  **There's no `if var` and no `if case`.** Use `match` for enum cases.
- **`guard … else { return }`** exits early, and its `else` must leave with `return`, `throw`, `break`
  or `continue`. Names bound by `guard let` stay in scope afterwards.
- **`match` is exhaustive** and is an expression. Its patterns are `.case`, `.case(let x)`,
  `.case(label: let x)`, several patterns in one arm (`case .a, .b`), `where` guards, literals, ranges
  (`0..<10`, `1...5`), tuples, `let x` and `_` inside patterns.
  - `default { … }` is the catch-all. It is required for numbers and strings, and an error when it's
    unreachable. A top-level `case _` is K2303, so write `default`.
  - `case let other { … }` binds the rest, and must use `other` (K2304 otherwise).
  - A bare name in a pattern is an error. Write `let x` to bind it, or `.x` for an enum case.
- **`for x in collection`**, `for i in 1...3` (closed), `for i in 0..<n` (half-open), with `break`
  and `continue`. **In a view, `for` needs a `key`:** `for item in items key item.id { … }` (K3106).
- **`for await x in stream`** reads a `Stream<T>`, and only inside an `async` function.
- `while condition { … }`. There's no `defer` and no `finally`.

```k
var delay = 1.seconds

while delay < 1.minutes {
    delay = delay * 2
}
```

## Strings

- `"Hello, {name}"` interpolates any one-line expression, including
  `"{if isSignedIn { "in" } else { "out" }}"`.
- A literal brace is `{{`, so `"Use {{name}} as a placeholder"` gives `Use {name} as a placeholder`. A
  closing `}` needs no escape, and `\{` is K0007.
- The escapes are `\n \r \t \0 \\ \" \u{1F44B}`. Strings are single-line, and multi-line literals
  aren't in the language.
- Interpolated values are inserted as text, never as HTML. A `Secret<T>` interpolates as `Secret(•••)`.
- Format with `value.formatted(as: .currency)`, which works on numbers, money and dates.

## Lambdas

```k
let add: (Int, Int) -> Int = { first, second => first + second }
let price = { (product: Item) -> Money => product.price }
let labels = items.map { item =>
    let amount = item.price * item.quantity

    "{item.quantity} × {amount.formatted(as: .currency)}"
}
```

- `=>` starts a lambda's body and `->` introduces a return type.
- There's no `$0` and no `it`, and `return` in a lambda is K2308. The last expression is its value.
- A trailing lambda follows the call (`clock.every(30.seconds) { refresh() }`). A call has at most one
  trailing lambda, and further function arguments are chained (`.row { item => … }`).
- In an `if`, `guard` or `while` condition, wrap a call that has a trailing lambda in parentheses:
  `if (items.contains { item => item.isNew }) { … }`.
- Pass a named function where a lambda is expected: `products.map(formatPrice)`,
  `.on(click: save)`.

## Collections

```k
let names: List<String> = ["Ana", "Ben"]
let ages: Map<String, Int> = ["Ana": 34, "Ben": 29]
let empty: Map<String, Int> = [:]
let tags: Set<String> = Set(["new", "sale"])

let first = names[0]
let anasAge = ages.get("Ana") ?? 0

let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }
let productNames = products.map { product => product.name }
let total = items.sum(in: .usd) { item => item.price * item.quantity }
let cheapest = products.minBy { product => product.price }
let byCategory = products.groupedBy { product => product.category }
let pages = products.chunked(20)
let soldOut = products.firstWhere { product => product.stock == 0 }
let hasSale = tags.contains("sale")

var growing = [1, 2, 3]
growing.append(4)
```

- Collections have **value semantics**: a `let` list can't change (K2011), and a copy never affects
  the original.
- Out-of-range indexing panics. Prefer `firstWhere`, `get`, `minBy` and friends, which return
  optionals.
- The API uses full names: `map`, `filter`, `sortedBy`, `sortedByDescending`, `groupedBy`,
  `partitioned`, `chunked`, `windowed`, `firstWhere`, `contains`, `minBy`, `maxBy`, `sum`, `count`,
  `isEmpty`, `isNotEmpty` and `append`. A `Money` sum names its currency with `sum(in: .usd)`.
- Tuples, `(Int, Int)`, are taken apart with patterns. There's no `tuple.0`.
- `Deque`, `LruCache` and the rest of `k.collections` are **Planned**, not built.

## Units, durations and money

- CSS units are literal suffixes: `16px`, `1.5rem`, `50%`, `200ms`, `2s`, `45deg`, `1fr` and
  `#3B82F6`. Mixing kinds (`100px + 200ms`) is K2101. Ordering two `Length`s (`100px > 2rem`) is K2013.
- Everything else is an extension property: `10.minutes`, `3.days`, `1.hour`, `25.megabytes`.
  `200ms == 0.2.seconds`.
- Money is exact: `9.99.usd`, `10.eur`. It never mixes currencies silently.
- **There's no `Date.now`.** The current time is `clock.now` from an injected `Clock`, so
  "two hours ago" is `clock.now - 2.hours`.
- With `%` right after a number it becomes a percent, so write `10 % 3` with spaces.

## Operators (low to high precedence)

`try`/`await` < `or` < `and` < `not` < comparisons (non-chaining) < `??` (right-assoc) < `..<` `...`
< `+ -` < `* / %` < unary `-` < calls, `.`, `?.` and `[ ]`. Assignment (`=`, `+=`, …) is a
statement, never an expression. `bind x` and `.a when condition` appear only as arguments.

## Keywords worth knowing

- **Hard:** `and or not as is async await bind break continue case catch class default do else enum
  expect extend false true nil for func guard if implements import in init interface let match
  private public requires return rpc self static struct style throw throws try use var when where
  while`.
- **Contextual**, at the start of a declaration or member: `app at component config entity failed
  key layout loading metadata migration page protocol repository service test theme variant view`.
  Elsewhere they are ordinary names (`let page = …` is fine).
