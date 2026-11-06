// A declaration without `:` reports K0100 at the token where `:` was expected and still parses the value.
component T {
    style {
        .a { color red; margin: 0; }
    }
}
