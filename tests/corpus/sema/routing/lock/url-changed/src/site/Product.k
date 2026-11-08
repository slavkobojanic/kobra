import k.crypto.Uuid

/// The Product page.
public page Product {
    @Param let id: Uuid
    @Param let variant: String

    view {
        paragraph { "Product" }
    }
}
