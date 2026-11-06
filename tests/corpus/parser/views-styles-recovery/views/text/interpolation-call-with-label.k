// An interpolation holding a method call with a labeled argument and an implicit member.
component T {
    view {
        button { "Pay {total.formatted(as: .currency)}" }
    }
}
