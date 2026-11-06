// Pseudo-class names may contain hyphens in style mode.
component T {
    style {
        .a:focus-visible {}
        .b:first-child:not-allowed {}
    }
}
