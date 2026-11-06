// Using a `Color` token for `padding` is a compile error (07-themes-and-units.md):
// the token exists, but not among the property's types (K3202 at the value).
theme Tokens {
    space md: Spacing = 16px
    color primary: Color = #3B82F6
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: .primary; color: .primary; margin: .md; }
    }
}
