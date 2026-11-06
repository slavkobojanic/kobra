// Nested rules using `&` with a pseudo-class, a class, a descendant and a pseudo-element.
component T {
    style {
        .a {
            &:hover {}
            &.active {}
            & .child {}
            &::before {}
        }
    }
}
