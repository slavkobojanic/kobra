// Tokens work in code too: an implicit member against `Color` finds the token.
theme Tokens {
    color primary: Color = #3B82F6
}

component Swatch {
    @Prop let tint: Color = .primary
    //                       ^^^^^^^ resolves: token Color.primary

    view {
        text { "x" }
    }
}
