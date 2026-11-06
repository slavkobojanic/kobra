// A missing `}` for an element shifts brace matching: the later braces close the element and the view, and only the component is unclosed.
component T {
    view {
        box {
            text { "x" }
    }
}
