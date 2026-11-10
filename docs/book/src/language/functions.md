# Functions

<!-- The types the samples in this chapter use; checked, not rendered.

```k
// src/catalog/Context.k
struct ProductId implements Equatable, Hashable {
    let value: Int
}

struct Review {
    let stars: Int
}

interface Priced {
    let price: Money
}

struct Product implements Priced {
    let id: ProductId
    let name: String
    let price: Money
}

enum CatalogError implements Error {
    case notFound
}

interface ReviewRepository {
    func reviews(productId: ProductId) async -> List<Review>
}

interface ProductStore {
    func find(_ id: ProductId) async throws CatalogError -> Product
}
```
-->

## Declaring and calling functions

Functions are declared with `func`. Parameters have names and types, and `->` introduces the return
type:

```k
func greeting(name: String) -> String {
    return "Hello, {name}!"
}

let message = greeting(name: "Ana")
```

A function with a return type returns its value with `return`. A function without one returns nothing:

```k
~import k.observe.Log
func logVisit(page: String) {
    Log.info("visited {page}")
}
```

## Argument labels

By default, **every argument is labeled with its parameter name**, so calls read clearly:
`greeting(name: "Ana")`, `cart.add(product: product)`.

A parameter can declare a separate **external label** before its name, as in Swift. The label is what
callers write; the name is what the body uses:

```k
~struct Reviews {
~    let repository: ReviewRepository
~
func reviews(for product: ProductId) async -> List<Review> {
    return await repository.reviews(productId: product)
}
~
~    func example(id: ProductId) async {
let found = await reviews(for: id)
~    }
~}
```

Labels may be keywords such as `for`, `in` and `as`, which often make calls read as prose. With a single
word, label and name are the same: `func greeting(name: String)` is called as `greeting(name: "Ana")`.

`_` instead of a label makes the argument positional. Use it when the function name already says what
the argument is:

```k
~func example(products: ProductStore, id: ProductId) async throws CatalogError {
let product = try await products.find(id)
~}
```

`ProductStore` declares `find` as `func find(_ id: ProductId) async throws CatalogError -> Product`.

## Default values

Parameters can have default values, and callers may leave them out:

```k
func formatted(amount: Decimal, decimals: Int = 2, grouping: Bool = true) -> String {
    // …
~    return "{amount}"
}

~func example(price: Decimal) {
~    let short =
formatted(amount: price)
~    let whole =
formatted(amount: price, decimals: 0)
~}
```

Plain functions use default parameters. Built-in elements, components, pages and `@Builder` types use
[chained calls](../apps/components.md#required-arguments-are-named-optional-ones-are-chained) for their
optional values instead.

## Effects: `async` and `throws`

A function's signature says whether it can suspend and what it can throw, always in this order:

```k
~let store: ProductStore? = nil
~
func load(id: ProductId) async throws CatalogError -> Product {
    // …
~    guard let store else {
~        throw .notFound
~    }
~    return try await store.find(id)
}
```

| Keyword | Meaning | At the call site |
|---|---|---|
| `async` | The function can suspend, for example to wait for the network | `await` |
| `throws E` | The function can fail with an error of type `E` | `try` |

```k
~func load(id: ProductId) async throws CatalogError -> Product {
~    throw .notFound
~}
~
~func example(id: ProductId) async throws CatalogError {
let product = try await load(id: id)
~}
```

`await` and `try` mark every point in your code where execution can pause or jump. `throws` always
names its error type; there is no untyped `throws`. The [Errors](errors.md) chapter covers this in
depth.

## Generic functions

Type parameters go in angle brackets after the name. Constraints go in a `where` clause, using the same
`implements` keyword as conformances:

```k
func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
}
```

Type arguments are inferred at the call site, or come from the type of the binding. When neither says
enough, for a function whose type parameter appears only in its result, write them after the name:

```k
~import k.encoding.Json
~import k.http.DecodingError
~
~func cheapest<T>(items: List<T>) -> T? where T implements Priced {
~    return items.minBy { item => item.price }
~}
~
~struct Draft implements Codable {
~    let title: String
~}
~
~func example(products: List<Product>, text: String) throws DecodingError {
let best = cheapest(items: products)       // T is Product
let tags: Set<String> = Set(["new"])       // the binding gives the type argument
let draft = try Json.decode<Draft>(text)   // written out
let first = cheapest<Product>(items: [])   // also checked against the constraint
~}
```

The type arguments follow the name directly, with no space before the `<`, and each starts with an
uppercase name, so `a < b` stays a comparison. See [Types](types.md#generics) for the rest.

## Functions as values

Functions are values. You can pass one by name wherever a function of the right type is expected:

```k
func formatPrice(product: Product) -> String {
    return product.price.formatted(as: .currency)
}
~
~component PriceList {
~    @Prop let products: List<Product>
~
let labels = products.map(formatPrice)
~
~    func save() {}
~
~    view {
button { "Save" }
    .on(click: save)
~    }
~}
```

Function types are written with parameter types in parentheses:

```k
~func load(id: ProductId) async throws CatalogError -> Product {
~    throw .notFound
~}
~
let transform: (Int) -> Int = { value => value * 2 }
let fetcher: (ProductId) async throws CatalogError -> Product = load
```

A function type's error may be generic. A function that takes a block typed `() async throws E -> T`
throws whatever the block passed to it throws, and a block that can't throw makes the call non-throwing,
so it needs no `try`:

```k,ignore
func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T

let saved = await database.transaction { await notes.update(edited) }      // can't throw
```

This is the only way to pass an error through. See
[Errors](errors.md#functions-that-pass-errors-through).

## Methods

Functions declared inside a type are its methods. Inside them, `self` refers to the instance, but you
rarely need to write it: fields and other methods are in scope.

```k
struct Rectangle {
    let width: Double
    let height: Double

    func area() -> Double {
        return width * height
    }

    func isLarger(than other: Rectangle) -> Bool {
        return area() > other.area()
    }
}
```

## Computed properties

A `let` or `var` with a block instead of an initializer is a **computed property**. Its last expression
is its value:

```k
struct Rectangle {
    let width: Double
    let height: Double

    var isSquare: Bool { width == height }
}
```

Computed properties are also how [extensions](declarations.md#extensions) add properties such as
`10.minutes` to existing types.
