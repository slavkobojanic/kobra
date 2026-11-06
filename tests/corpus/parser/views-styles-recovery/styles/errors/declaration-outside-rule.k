// A declaration directly in the style block is K0133; the span covers the whole Declaration including `;`.
component T {
    style {
        color: red;
        .a { color: blue; }
    }
}
