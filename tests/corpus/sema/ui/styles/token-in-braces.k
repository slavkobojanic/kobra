// Theme tokens never need braces: `{.primary}` is K3210 with a machine-applicable
// fix that removes them.
theme Tokens {
    color primary: Color = #3B82F6
}

component Summary {
    view {
        box { "x" }
            .class(.summary)
    }

    style {
        .summary { color: {.primary}; }
    }
}
