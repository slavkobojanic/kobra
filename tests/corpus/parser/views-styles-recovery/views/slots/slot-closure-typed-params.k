// A slot closure with a parenthesized, typed parameter list.
component T {
    view {
        List(items: items)
            .row { (item: CartItem) => CartRow(item: item) }
    }
}
