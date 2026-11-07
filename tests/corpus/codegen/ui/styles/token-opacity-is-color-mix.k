// `.primary.opacity(0.8)` on a token is a `color-mix()` over the token's custom property, so it
// follows variants (codegen.md §6.5).
theme Tokens {
    color primary: Color = #3B82F6
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { borderLeft: 4px solid .primary.opacity(0.8); }
    }
}
