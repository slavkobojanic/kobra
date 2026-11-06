// Arguments spread over several lines, with a trailing comma; newlines inside parentheses are insignificant.
component T {
    view {
        CartRow(
            item: item,
            quantity: bind item.quantity,
        )
    }
}
