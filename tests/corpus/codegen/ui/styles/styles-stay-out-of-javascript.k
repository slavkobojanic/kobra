// Styles live only in the stylesheet: no CSS text reaches the JS bundles.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: 13px; }
    }
}
