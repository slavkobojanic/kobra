// No zero-argument flag methods: `.disabled()` misses its `Bool` (K2003).
component Pay {
    view {
        button { "Pay" }
            .disabled()
    }
}
