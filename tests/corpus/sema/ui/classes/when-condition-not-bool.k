// The `when` condition must be a `Bool` (K2001 at the condition).
component Panel {
    @State var count: Int = 0

    view {
        box { "x" }
            .class([.panel, .wide when count])
    }

    style {
        .panel { padding: 4px; }
        .wide { width: 100%; }
    }
}
