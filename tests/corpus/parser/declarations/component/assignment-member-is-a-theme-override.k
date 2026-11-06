// The parser accepts every member kind in every body (grammar.md §9): an
// assignment at member level is a `ThemeOverride`, which semantic analysis
// rejects outside a theme.
component C {
    count = 5
    @State var total: Int = 0
}
