// `/product/{id}` (the default URL of `Product`) and `/product/{slug}` match the same paths.
page Product {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}

page ProductBySlug at "/product/{slug}" {
    @Param let slug: String

    view {
        text { slug }
    }
}
