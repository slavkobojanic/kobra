// A single class may be conditional without a list.
component Panel {
    @State var isWide: Bool = false

    view {
        box { "x" }
            .class(.wide when isWide)
    }

    style {
        .wide { width: 100%; }
    }
}
