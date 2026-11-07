// A page with a required parameter has it as a path segment: the link's `href` is built from the
// generated `Page` case at run time (12-routing.md).
page Product {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}

component ProductLink {
    @Prop let id: Int

    view {
        link(to: .product(id: id)) { "Product" }
    }
}
