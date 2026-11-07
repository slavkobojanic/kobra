// A load that reads only parameters has no `after`, so it starts at once, in parallel with the
// others (14-async-data.md, "The loading order comes from the dependency graph").
struct Item implements Codable {
    let id: Int
}

service Catalog {
    func find(id: Int) async -> Item {
        return Item(id: id)
    }

    func related(id: Int) async -> List<Item> {
        return []
    }
}

page ItemPage {
    @Param let id: Int
    @Inject let catalog: Catalog

    @Load let item: Item = await catalog.find(id: id)
    @Load let related: List<Item> = await catalog.related(id: id)

    view {
        text { "{item.id} {related.count}" }
    }

    loading {
    }
}
