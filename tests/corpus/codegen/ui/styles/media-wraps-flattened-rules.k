// `@media` inside a rule wraps the flattened rule; media features are kebab case too
// (codegen.md §12, "Rules").
theme Tokens {
    space sm: Spacing = 8px
    space md: Spacing = 16px
}

component Summary {
    view {
        section { "x" }
            .class(.summary)
    }

    style {
        .summary {
            padding: .md;

            @media (maxWidth: 600px) { padding: .sm; }
        }
    }
}
