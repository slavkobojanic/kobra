// The Card example: trailing content block, then chained named slots `.row { item => ... }` and `.footer { ... }`.
component T {
    view {
        Card(title: "Your cart", items: items) {
            paragraph { "Review before checkout." }
        }
        .row { item =>
            CartRow(item: item)
        }
        .footer {
            button { "Checkout" }
                .on(click: checkout)
        }
    }
}
