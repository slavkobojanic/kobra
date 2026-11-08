import k.crypto.Uuid

/// The Product page.
public page Product {
    @Param let id: Uuid

    view {
        paragraph { "Product" }
    }
}
