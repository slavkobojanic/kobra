// Class references inside `for` and `if` in a view count as uses.
component Names {
    @Prop let names: List<String>
    @State var isEmpty: Bool = false

    view {
        for name in names key name {
            text { name }
                .class(.name)
        }

        if isEmpty {
            paragraph { "none" }
                .class(.empty)
        }
    }

    style {
        .name { fontWeight: bold; }
        .empty { color: #999999; }
    }
}
