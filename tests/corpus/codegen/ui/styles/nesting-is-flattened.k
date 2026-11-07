// `&:hover` and `&::before` nested in a rule become flattened selectors on the scoped class
// (codegen.md §12, "Rules").
component Link {
    view {
        box { "x" }
            .class(.item)
    }

    style {
        .item {
            color: #000000;

            &:hover { color: #FF0000; }

            &::before { color: #00FF00; }
        }
    }
}
