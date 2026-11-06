// An event handler lambda with a parameter.
component T {
    view {
        textField(value: query)
            .on(input: { event => query = event.value })
    }
}
