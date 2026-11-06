// The universal selector alone and after a child combinator.
component T {
    style {
        * { margin: 0; }
        .a > * {}
    }
}
