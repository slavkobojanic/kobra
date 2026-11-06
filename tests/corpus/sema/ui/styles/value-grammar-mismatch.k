// Values are checked against the property's CSS grammar: `borderLeft: 4px blue
// solid potato` fails at `potato` (06-styles.md), K3202 at the first component
// that doesn't match.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { borderLeft: 4px #0000FF solid potato; }
    }
}
