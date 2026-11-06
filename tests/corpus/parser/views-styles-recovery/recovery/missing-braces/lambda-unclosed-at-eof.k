// End of file inside a slot closure: each open brace reports K0101.
component T {
    view {
        List(items: items)
            .row { item =>
                CartRow(item: item)
