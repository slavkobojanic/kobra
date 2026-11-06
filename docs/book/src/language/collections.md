# Collections

K has three core collections: `List`, `Map` and `Set`. They need no import.

```k
let names: List<String> = ["Ana", "Ben"]
let prices: Map<ProductId, Money> = [id1: 9.99.usd, id2: 14.50.usd]
let tags: Set<String> = Set(["new", "sale"])
```

## Value semantics

Collections behave like [structs](declarations.md#structs): **assigning or passing one behaves like a
copy.** A collection declared with `let` can't change; one declared with `var` can:

```k
let fixed = [1, 2, 3]
fixed.append(4)            // compile error: fixed is a let

var growing = [1, 2, 3]
growing.append(4)          // [1, 2, 3, 4]

var copy = growing
copy.append(5)             // growing is still [1, 2, 3, 4]
```

The compiler uses copy-on-write, so the JavaScript it emits doesn't actually clone a collection unless
one of the copies changes.

This matters most for reactive state. Mutating a `@State` list is tracked like any other write, and only
the parts of the view that read it update:

```k
@State var items: List<CartItem> = []

func add(item: CartItem) {
    items.append(item)
}
```

## Literals

| Literal | Type |
|---|---|
| `[a, b, c]` | `List` |
| `[]` | an empty `List`; the element type must be known |
| `[key: value, other: value]` | `Map` |
| `[:]` | an empty `Map` |
| `Set([a, b])` | `Set` |

Elements and entries may span lines, with a trailing comma:

```k
let palette: List<Color> = [
    .primary,
    .surface,
    .muted,
]
```

## Reading elements

```k
let first = names[0]                       // "Ana"
let count = names.count                    // 2
let isEmpty = names.isEmpty                // false
let hasAny = names.isNotEmpty              // true
```

Reading a list index that is out of range is a programmer error, so it **panics** rather than throwing.
Prefer the collection API, which returns optionals where an element might be missing:

```k
let firstAdmin = users.firstWhere { user => user.isAdmin }      // User?
```

## The collection API

The standard library's collection API is extensive and uses full English names. Most operations take a
[lambda](lambdas.md):

```k
let inStock = products.filter { product => product.stock > 0 }
let names = products.map { product => product.name }
let byPrice = products.sortedBy { product => product.price }
let total = items.sum { item => item.price * item.quantity }
let cheapest = products.minBy { product => product.price }
let byCategory = products.groupedBy { product => product.category }
let pages = products.chunked(20)
```

| Kind | Operations |
|---|---|
| Transforming | `map`, `filter`, `sortedBy`, `sortedByDescending`, `groupedBy`, `partitioned`, `chunked`, `windowed` |
| Searching | `firstWhere`, `contains`, `minBy`, `maxBy` |
| Aggregating | `sum`, `count`, `isEmpty`, `isNotEmpty` |
| Changing (on a `var`) | `append`, and the other mutating operations |

There are no abbreviated or symbolic aliases. The same API is used for
[database queries](../apps/persistence.md), where the compiler translates it to SQL.

## Iterating

`for … in` visits each element in order:

```k
for name in names {
    Log.info("hello, {name}")
}
```

In a view, a `for` loop renders one node per element and requires a `key`, which identifies each element
across updates:

```k
for item in items key item.id {
    CartRow(item: item)
}
```

See [Control flow](control-flow.md#for-loops).

## More collections

The `k.collections` module adds specialized types: `Deque`, `PriorityQueue`, `OrderedMap` and
`LruCache`. They are imported like any other declaration:

```k
import k.collections.LruCache
```
