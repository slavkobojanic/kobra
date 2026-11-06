# Functions

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
func reviews(for product: ProductId) async -> List<Review> {
    return await repository.reviews(productId: product)
}

let found = await reviews(for: id)
```

Labels may be keywords such as `for`, `in` and `as`, which often make calls read as prose.

`_` instead of a label makes the argument positional. Use it when the function name already says what
the argument is:

```k
func find(_ id: ProductId) async throws CatalogError -> Product { … }

let product = try await products.find(id)
```

## Default values

Parameters can have default values, and callers may leave them out:

```k
func formatted(amount: Decimal, decimals: Int = 2, grouping: Bool = true) -> String { … }

formatted(amount: price)
formatted(amount: price, decimals: 0)
```

Plain functions use default parameters. Built-in elements, components, pages and `@Builder` types use
[chained calls](../apps/components.md#required-arguments-are-named-optional-ones-are-chained) for their
optional values instead.

## Effects: `async` and `throws`

A function's signature says whether it can suspend and what it can throw, always in this order:

```k
func load(id: ProductId) async throws CatalogError -> Product { … }
```

| Keyword | Meaning | At the call site |
|---|---|---|
| `async` | The function can suspend, for example to wait for the network | `await` |
| `throws E` | The function can fail with an error of type `E` | `try` |

```k
let product = try await load(id: id)
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

Type arguments are always inferred at the call site, or come from the type of the binding:

```k
let best = cheapest(items: products)       // T is Product
```

## Functions as values

Functions are values. You can pass one by name wherever a function of the right type is expected:

```k
func formatPrice(product: Product) -> String {
    return product.price.formatted(as: .currency)
}

let labels = products.map(formatPrice)

button { "Save" }
    .on(click: save)
```

Function types are written with parameter types in parentheses:

```k
let transform: (Int) -> Int = { value => value * 2 }
let fetcher: (ProductId) async throws CatalogError -> Product = load
```

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
