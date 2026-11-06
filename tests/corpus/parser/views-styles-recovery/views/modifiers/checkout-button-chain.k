// Three chained attributes and an event, each on its own line (from the views design).
component T {
    view {
        button { "Pay {total}" }
            .type(.submit)
            .disabled(isSubmitting)
            .on(click: checkout)
    }
}
