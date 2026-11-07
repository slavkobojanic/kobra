import k.crypto.Uuid

/// A `Uuid` parameter: anything that isn't a UUID is a 404.
public page Item {
    @Param let id: Uuid

    view {
        paragraph { "item {id}" }
    }
}
