// Type selectors after a combinator and glued to `&` are each reported.
component T {
    style {
        .a > button {}
        .b {
            &span {}
        }
    }
}
