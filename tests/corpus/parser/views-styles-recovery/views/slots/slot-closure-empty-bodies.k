// Slot closures with empty bodies, with and without parameters.
component T {
    view {
        List(items: items)
            .empty {}
            .row { item => }
    }
}
