// A list of classes, with `when` for conditional ones; the condition is a `Bool`.
component Panel {
    @State var isExpanded: Bool = false

    view {
        box { "x" }
            .class([.panel, .expanded when isExpanded])
            //               ^^^^^^^^ resolves: class corpus.Panel.expanded
            //                             ^^^^^^^^^^ type: Bool
    }

    style {
        .panel { maxHeight: 0; overflow: hidden; }
        .expanded { maxHeight: 400px; }
    }
}
