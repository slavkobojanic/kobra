// Token values exist only in the stylesheet; switching a variant swaps one class and re-renders
// nothing (07-themes-and-units.md).
theme Tokens {
    color accent: Color = #ABCDEF
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { color: .accent; }
    }
}
