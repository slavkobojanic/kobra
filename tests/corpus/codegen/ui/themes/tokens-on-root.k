// Theme tokens become custom properties on `:root`, in declaration order, named by category and
// token in kebab case; the stylesheet starts with the `:root` block (codegen.md §12, "Theme tokens").
theme Tokens {
    color primary: Color = #3B82F6
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
        .card { padding: .md; }
    }
}
