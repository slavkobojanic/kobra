// A token no theme declares is K2006 at the name, with the closest token of the
// expected categories as a fix.
theme Tokens {
    space md: Spacing = 16px
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: .mdd; }
    }
}
