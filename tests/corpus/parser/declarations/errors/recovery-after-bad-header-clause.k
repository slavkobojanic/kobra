// After a header error, the body and later items still parse.
page A requires {
    @Param let id: Int
}
page B requires Authenticated {}
