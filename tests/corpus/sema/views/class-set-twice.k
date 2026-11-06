component Card {
    view {
        box { "x" }
            .class(.card)
            .class(.wide)
        box { "y" }
            .class([.card, .wide])
    }

    style {
        .card { padding: 16px; }
        .wide { width: 100%; }
    }
}
