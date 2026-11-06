// A class that appears in several selectors is one class: unused, it gets one
// K3204, at its first selector.
component Menu {
    view {
        box { "x" }
            .class(.list)
    }

    style {
        .list { padding: 4px; }
        .dim { opacity: 0.5; }
        .list .dim { opacity: 0.2; }
    }
}
