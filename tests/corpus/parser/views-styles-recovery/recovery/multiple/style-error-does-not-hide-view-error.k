// A style error before the view does not hide the view's error.
component T {
    style {
        color: red;
    }

    view {
        text { "a" } text { "b" }
    }
}
