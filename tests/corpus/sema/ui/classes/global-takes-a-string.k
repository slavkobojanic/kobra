// `global(_:)` takes a `String` (K2001 at the argument).
component Pay {
    view {
        button { "Pay" }
            .class(global(42))
    }
}
