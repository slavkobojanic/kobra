// Reactive values interpolated with `{ }`: a name, a member path and an arithmetic expression.
component T {
    style {
        .a {
            borderLeft: 4px solid {accent};
            color: {theme.accent};
            height: {rowHeight * count};
        }
    }
}
