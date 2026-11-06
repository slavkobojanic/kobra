// An `at` template mentions each path `@Param` once; optional parameters may be put in the path
// explicitly.
enum ProductTab {
    case overview
    case reviews
}

page ProductDetail at "/p/{slug}/{tab}" {
    @Param let slug: String
    @Param let tab: ProductTab = .overview

    view {
        text { slug }
    }
}
