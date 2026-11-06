// `.class()` twice is K3104 (use a list); one list holding the same classes is
// fine.
component Card {
    view {
        box { "x" }
            .class(.card)
            .class(.wide)
        box { "y" }
            .class([.card, .wide])
    }

    style {
        .card { padding: 4px; }
        .wide { width: 100%; }
    }
}
