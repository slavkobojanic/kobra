// A page with required and optional params, an injection and a view.
page Product requires Authenticated {
    @Param let id: ProductId
    @Param let tab: ProductTab = .overview

    @Inject let products: ProductService

    view {
        heading(level: 1) { "Product {id}" }
    }
}
