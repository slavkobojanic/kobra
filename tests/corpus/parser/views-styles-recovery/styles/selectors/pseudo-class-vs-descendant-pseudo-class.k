// `.a:hover` is one compound selector; `.a :hover` is two (whitespace is significant inside selectors).
component T {
    style {
        .a:hover {}
        .a :hover {}
    }
}
