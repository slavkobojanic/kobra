// `padding` takes one to four lengths: a fifth is K3202 at the fifth.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: 1px 2px 3px 4px 5px; margin: 1px 2px 3px 4px; }
    }
}
