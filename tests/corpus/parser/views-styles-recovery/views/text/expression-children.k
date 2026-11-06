// Non-string expressions as children: a name and a method chain.
component T {
    view {
        heading(level: 3) { item.name }

        text { item.price.formatted(as: .currency) }
    }
}
