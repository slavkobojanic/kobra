// An unresolved `@Param` type reports K1202 only, not also K5002.
page Product {
    @Param let id: ProductIdd

    view {
        text { "product" }
    }
}
