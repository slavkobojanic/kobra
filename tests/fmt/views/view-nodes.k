component Card {
    @Prop let title: String
    @Prop let items: List<String>

    view {

        article {
            heading(level: 2) { title }
            let count = items.count
            let label = "{count} items"
            paragraph { label }
            for item in items key item {
                text { item }
                text { "!" }
            }
            if items.isEmpty {
                "Nothing here yet."
                link(to: .home) { "Home" }
            } else {
                "Some"
            }
        }

    }
}
