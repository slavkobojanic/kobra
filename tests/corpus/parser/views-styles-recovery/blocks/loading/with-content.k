// A loading block holding view nodes.
page Product {
    loading {
        ProductSkeleton()

        text { "Loading…" }
            .class(.muted)
    }
}
