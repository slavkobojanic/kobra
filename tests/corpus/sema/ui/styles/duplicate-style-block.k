// At most one `style` block per component (K3004 at the second, label at the
// first).
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: 4px; }
    }

    style {
        .card { margin: 4px; }
    }
}
