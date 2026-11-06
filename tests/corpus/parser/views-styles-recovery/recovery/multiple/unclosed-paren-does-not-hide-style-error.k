// An unclosed argument list in the view does not hide a later style error.
component T {
    view {
        Spinner(
    }

    style {
        .a { color: red !important; }
    }
}
