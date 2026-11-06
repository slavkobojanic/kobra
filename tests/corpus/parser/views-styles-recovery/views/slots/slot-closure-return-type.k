// A slot closure with a typed parameter and a return type.
component T {
    view {
        List(items: items)
            .row { (item: CartItem) -> View => CartRow(item: item) }
    }
}
