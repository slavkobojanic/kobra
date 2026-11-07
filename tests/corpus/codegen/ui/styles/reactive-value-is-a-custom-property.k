// `{accent}` in a rule is a component-scoped custom property: the CSS reads `var(--k-…-0)`, the
// element gets a `style` binding, and the server writes the property in the `style` attribute
// (06-styles.md, codegen.md §12, "Reactive values").
theme Tokens {
    color primary: Color = #3B82F6
}

component Summary {
    @Prop let accent: Color = .primary

    view {
        section { "x" }
            .class(.summary)
    }

    style {
        .summary { borderLeft: 4px solid {accent}; }
    }
}
