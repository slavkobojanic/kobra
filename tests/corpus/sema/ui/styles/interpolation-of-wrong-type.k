// An interpolated value of a type the property doesn't accept is K3202 at the
// interpolation.
component Summary {
    @State var count: Int = 0

    view {
        box { "x" }
            .class(.summary)
    }

    style {
        .summary { color: {count}; }
    }
}
