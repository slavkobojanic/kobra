// A page parameter has its `@Param` field's type.
page Product {
    @Param let id: Int

    view {
        link(to: .product(id: "one")) { "Product" }
        link(to: .product(id: 1)) { "Product" }
    }
}
