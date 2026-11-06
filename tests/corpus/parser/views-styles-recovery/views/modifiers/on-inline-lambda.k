// An inline event handler lambda containing an assignment statement.
component T {
    view {
        button { "+" }
            .on(click: { value += step })
    }
}
