// `!important` is K0118 on `!important`; the token pair is wrapped in an ErrorNode inside the declaration.
component T {
    style {
        .a { color: red !important; }
    }
}
