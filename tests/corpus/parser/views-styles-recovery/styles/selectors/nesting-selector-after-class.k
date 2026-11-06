// `&` may appear after another compound selector, and twice in one selector.
component T {
    style {
        .a {
            .dark & {}
            & + & {}
        }
    }
}
