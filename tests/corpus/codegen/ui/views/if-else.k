// `if`/`else` passes the else branch as `otherwise` (runtime.md §5.3).
component Cart {
    @State var items: List<String> = []

    view {
        box {
            if items.isEmpty {
                paragraph { "Nothing here yet." }
            } else {
                paragraph { "Ready to pay" }
            }
        }
    }
}
