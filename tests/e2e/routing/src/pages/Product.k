/// A required `Int` in the path and an optional enum in the query.
public page Product {
    @Param let id: Int
    @Param let tab: ProductTab = .overview

    view {
        paragraph { "product {id}" }

        if tab == .reviews {
            paragraph { "tab reviews" }
        } else {
            paragraph { "tab overview" }
        }

        link(to: .product(id: id).tab(.reviews)) { "Reviews" }
        link(to: .product(id: id)) { "Overview" }
        link(to: .productReviews(id: id)) { "All reviews" }
    }
}
