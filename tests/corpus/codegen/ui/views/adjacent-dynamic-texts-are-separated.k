// Two adjacent dynamic texts get a `<!---->` between them, because HTML would merge them into one
// text node; each has its own effect (codegen.md §11.1, runtime.md §6).
component FullName {
    @Prop let first: String
    @Prop let last: String

    view {
        paragraph {
            first

            last
        }
    }
}
