// A component with required named arguments, including a `bind` argument.
component T {
    view {
        CartRow(item: item, quantity: bind item.quantity)
    }
}
