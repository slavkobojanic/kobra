// Theme tokens are implicit members resolved against the property's types:
// `padding: .md` is `Spacing.md`, `borderRadius: .md` is `Radius.md`; the same
// name in two categories doesn't clash (07-themes-and-units.md).
theme Tokens {
    space md: Spacing = 16px
    radius md: Radius = 8px
    color surface: Color = #FFFFFF
    color primary: Color = #3B82F6
}

component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            padding: .md;
            //        ^^ resolves: token Spacing.md
            borderRadius: .md;
            //             ^^ resolves: token Radius.md
            backgroundColor: .surface;
            //                ^^^^^^^ resolves: token Color.surface
            borderLeft: 4px solid .primary.opacity(0.8);
            //                    ^^^^^^^^^^^^^^^^^^^^^ type: Color
        }
    }
}
