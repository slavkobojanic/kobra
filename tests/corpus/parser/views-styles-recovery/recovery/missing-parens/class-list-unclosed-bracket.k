// A class list missing its `]`: K0101 at the `[`; the `)` still closes the argument list.
component T {
    view {
        box {}
            .class([.a, .b)
    }
}
