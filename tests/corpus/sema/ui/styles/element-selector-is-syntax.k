// Element selectors are syntax errors (K0116, RA-8); semantic analysis adds
// nothing for them.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: 4px; }
        button { color: red; }
    }
}
