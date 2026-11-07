# Lambdas

<!-- The types the samples in this chapter use; checked, not rendered.

```k
// src/shop/Context.k
import k.time.Clock

struct Product {
    let name: String
    let stock: Int
    let price: Money
}

struct User {
    let isActive: Bool
}

struct OrderItem {
    let name: String
}

struct Order {
    let number: Int
    let items: List<OrderItem>
}

struct CartItem {
    let price: Money
    let isNew: Bool
}

component ProductCard {
    @Prop let product: Product

    view {
        text { product.name }
    }
}

component CartRow {
    @Prop let item: CartItem

    view {
        text { item.price.formatted(as: .currency) }
    }
}

component NewBadge {
    view {
        text { "New" }
    }
}

component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View
    @Slot let row: (CartItem) -> View

    view {
        heading(level: 2) { title }
        content
        for item in items key item.price {
            row(item)
        }
    }
}

func formatPrice(product: Product) -> String {
    return product.price.formatted(as: .currency)
}
```
-->

A lambda is an anonymous function written in braces. Its parameters come first, followed by `=>` and the
body:

```k
~func cards(products: List<Product>) -> List<View> {
~    return products
~        .filter { product => product.stock > 0 }
~        .sortedBy { product => product.price }
~        .map { product => ProductCard(product: product) }
~}
~
~let products: List<Product> = []
~
let inStock = products
    .filter { product => product.stock > 0 }
    .sortedBy { product => product.price }
    .map { product => ProductCard(product: product) }
```

## Parameters are always named

**Every lambda parameter has a name.** There is no implicit `it` and no `$0`. A name costs a few
characters and saves every reader from guessing:

```k
~func active(users: List<User>) -> List<User> {
~    return
users.filter { user => user.isActive }
~}
```

The shorthand `users.filter { $0.isActive }` from other languages is not valid K.

A lambda with several parameters separates them with commas:

```k
let add: (Int, Int) -> Int = { first, second => first + second }
```

A lambda without parameters has no arrow at all:

```k
~import k.time.Clock
~
~component Stepper {
~    @Inject let clock: Clock
~    @Binding var value: Int
~    @Prop let step: Int = 1
~
~    func refresh() {}
~
~    func start() {
clock.every(30.seconds) { refresh() }
~    }
~
~    view {
button { "+" }
    .on(click: { value += step })
~    }
~}
```

## The body's value

The last expression in a lambda's body is its value. **`return` inside a lambda is a compile error,** so
`return` only ever leaves a declared function:

```k
~func labels(orders: List<Order>) -> List<String> {
let labels = orders.map { order =>
    let count = order.items.count

    "{order.number}: {count} items"
}
~    return labels
~}
```

This holds for longer lambdas too, such as the block a transaction runs:

```k
// examples/shop/shop/src/store/PaymentRecorder.k
let recorded = await database.transaction {
    let marked = await orders.markPaid(id: order.id, reference: reference, paidAt: clock.now)

    if marked {
        await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: order.id))
    }

    marked
}
```

`marked`, the last expression, is the lambda's value, and so the value of `recorded`; writing
`return marked` there would be a compile error.

Other blocks used as values, such as an `if` or `match` arm used as an expression or a computed
property, also take their last expression as the value. Functions with a return type use `return`.

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
~import k.time.Clock
~
~service Ticker {
~    @Inject let clock: Clock
~
~    func refresh() {}
~
~    func start(items: List<CartItem>) -> Money {
clock.every(30.seconds) { refresh() }
~        return
items.sum(in: .usd) { item => item.price }
~    }
~}
```

This is the same mechanism that makes views read naturally. `box { … }` is a call to `box` with a
trailing lambda, and `heading(level: 2) { title }` has both arguments and a trailing lambda.

A call has at most one trailing lambda. Further function arguments are chained, as with a component's
named slots:

```k
~component CartPage {
~    @Prop let items: List<CartItem>
~
~    view {
Card(title: "Your cart", items: items) {
    paragraph { "Review before checkout." }
}
.row { item =>
    CartRow(item: item)
}
~    }
~}
```

In the condition of an `if`, `guard` or `while`, and after `for … in`, a `{` always starts the
statement's block. To use a trailing lambda there, wrap the call in parentheses:

```k
~component CartBadge {
~    @Prop let items: List<CartItem>
~
~    view {
if (items.contains { item => item.isNew }) {
    NewBadge()
}
~    }
~}
```

## Passing functions by name

Wherever a lambda is expected, you can pass a named function of the right type instead:

```k
~component PriceList {
~    @Prop let products: List<Product>
~
let prices = products.map(formatPrice)
~
~    func save() {}
~
~    view {
button { "Save" }
    .on(click: save)
~    }
~}
```

## Capturing

A lambda can read and write the names in scope where it was written. Writing `@State` from a lambda is
an ordinary state change and updates the view:

```k
~component Counter {
@State var count: Int = 0

view {
    button { "Add" }
        .on(click: { count += 1 })
}
~}
```
