// A declaration with nothing after `:` reports K0100 at the `;`; the value is absent from the tree.
component T {
    style {
        .a { color: ; margin: 0; }
    }
}
