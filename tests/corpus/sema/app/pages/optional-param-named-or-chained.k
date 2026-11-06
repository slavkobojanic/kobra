// Optional parameters are not part of the case's payload; they are set by name or chained.
enum ProductTab {
    case overview
    case reviews
}

page Product {
    @Param let id: Int
    @Param let tab: ProductTab = .overview

    view {
        link(to: .product(id: id).tab(.reviews)) { "Reviews" }
        //       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: Page
        link(to: .product(id: id, tab: .reviews)) { "Reviews" }
        //       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: Page
    }
}
