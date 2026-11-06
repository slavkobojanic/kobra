// A media feature without a value, and `not` in a prelude (an identifier in style mode).
component T {
    style {
        @media (prefersReducedMotion) {}
        @media not print {}
    }
}
