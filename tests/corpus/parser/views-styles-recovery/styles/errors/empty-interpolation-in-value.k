// An empty interpolation in a style value is K0128 on the `{}`.
component T {
    style {
        .a { width: {}; }
    }
}
