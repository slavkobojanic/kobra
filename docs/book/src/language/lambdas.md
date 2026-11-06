# Lambdas

A lambda is an anonymous function written in braces. Its parameters come first, followed by `=>` and the
body:

```k
let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }
    .map { product => ProductCard(product: product) }
```

## Parameters are always named

**Every lambda parameter has a name.** There is no implicit `it` and no `$0`. A name costs a few
characters and saves every reader from guessing:

```k
users.filter { user => user.isActive }
```

The shorthand `users.filter { $0.isActive }` from other languages is not valid K.

A lambda with several parameters separates them with commas:

```k
let add: (Int, Int) -> Int = { first, second => first + second }
```

A lambda without parameters has no arrow at all:

```k
Timer.every(30.seconds) { refresh() }

button { "+" }
    .on(click: { value += step })
```

## The body's value

The last expression in a lambda's body is its value. There is no `return` in a lambda:

```k
let labels = orders.map { order =>
    let count = order.items.count

    "{order.number}: {count} items"
}
```

## Types

Parameter and return types are normally inferred from where the lambda is used. When they can't be, use
the parenthesized form, with `->` for the return type:

```k
let price = { (product: Product) -> Money => product.price }
```

Remember the two arrows: **`=>` starts a lambda's body, `->` introduces a return type.**

## Trailing lambdas

When the last parameter of a function is a function, the lambda can follow the call's parentheses, or
replace them entirely:

```k
Timer.every(30.seconds) { refresh() }
items.sum(in: .usd) { item => item.price }
```

This is the same mechanism that makes views read naturally. `box { … }` is a call to `box` with a
trailing lambda, and `heading(level: 2) { title }` has both arguments and a trailing lambda.

A call has at most one trailing lambda. Further function arguments are chained, as with a component's
named slots:

```k
Card(title: "Your cart", items: items) {
    paragraph { "Review before checkout." }
}
.row { item =>
    CartRow(item: item)
}
```

In the condition of an `if`, `guard` or `while`, and after `for … in`, a `{` always starts the
statement's block. To use a trailing lambda there, wrap the call in parentheses:

```k
if (items.contains { item => item.isNew }) {
    NewBadge()
}
```

## Passing functions by name

Wherever a lambda is expected, you can pass a named function of the right type instead:

```k
let prices = products.map(formatPrice)

button { "Save" }
    .on(click: save)
```

## Capturing

A lambda can read and write the names in scope where it was written. Writing `@State` from a lambda is
an ordinary state change and updates the view:

```k
@State var count: Int = 0

view {
    button { "Add" }
        .on(click: { count += 1 })
}
```
