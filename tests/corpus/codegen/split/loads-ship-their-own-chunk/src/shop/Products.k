/// A page with a load, which imports the `load` chunk.
service Catalog {
    func count() async -> Int {
        return 3
    }
}

public page Products at "/products" {
    @Inject let catalog: Catalog

    @Load let count: Int = await catalog.count()

    view {
        heading(level: 1) { "Products" }
        text { "{count} products" }
    }

    loading {
        text { "Loading…" }
    }
}
