// `for tag in tags key tag` is an `each` block with a key function; rows are created by key and
// moved, not re-created (codegen.md §11.1, runtime.md §5.3).
component Tags {
    @State var tags: List<String> = []

    view {
        list {
            for tag in tags key tag {
                item { tag }
            }
        }
    }
}
