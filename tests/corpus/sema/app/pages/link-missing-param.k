// A page case without its required parameter is a missing argument, with or without parentheses.
page Product {
    @Param let id: Int

    view {
        link(to: .product()) { "Product" }
        link(to: .product) { "Product" }
    }
}
