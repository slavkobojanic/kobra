// Implicit members in values resolve to theme tokens and print as custom properties named
// `--k-<category>-<token>` in kebab case (codegen.md §12, "Values").
theme Tokens {
    color muted: Color = #6B7280
    color surfaceHover: Color = #F3F4F6
    space md: Spacing = 16px
    radius lg: Radius = 16px
    fontSize sm: FontSize = 0.875rem
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            color: .muted;
            backgroundColor: .surfaceHover;
            padding: .md;
            borderRadius: .lg;
            fontSize: .sm;
        }
    }
}
