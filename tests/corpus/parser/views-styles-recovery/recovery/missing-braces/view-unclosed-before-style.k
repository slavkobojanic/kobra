// A view block left open before a `style` member: `style` cannot start a statement, so the view block ends there (K0101 at its `{`) and the style block parses as a member.
component T {
    view {
        box { "x" }

    style {
        .a { color: red; }
    }
}
