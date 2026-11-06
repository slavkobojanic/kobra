// `.disabled(Bool)`: a string is K2001 at the value.
component Pay {
    view {
        button { "Pay" }
            .disabled("yes")
        button { "Pay" }
            .disabled(true)
    }
}
