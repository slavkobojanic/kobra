// A syntax error inside a view is reported by the parser; semantic analysis adds
// nothing for the broken node.
component Card {
    view {
        box { "x" }
            .class(.card
        text { "y" }
    }

    style {
        .card { padding: 4px; }
    }
}
