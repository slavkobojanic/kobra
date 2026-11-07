// Optional parameters go in the query string and are omitted at their default
// (12-routing.md, "Default URLs").
enum ProductTab {
    case overview
    case reviews
}

page Product {
    @Param let id: Int
    @Param let tab: ProductTab = .overview

    view {
        link(to: .product(id: id).tab(.reviews)) { "Reviews" }
    }
}
