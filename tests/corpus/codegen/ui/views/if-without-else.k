// `if` is a `when` block between markers with no `otherwise` (codegen.md §11.1, runtime.md §5.3).
component Notice {
    @State var isShown: Bool = false

    view {
        box {
            if isShown {
                paragraph { "Hello" }
            }
        }
    }
}
