// Pages whose URLs differ don't conflict, query parameters included.
page Search {
    @Param let query: String = ""

    view {
        text { query }
    }
}

page SearchHelp {
    view {
        text { "help" }
    }
}

page Product {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}

page ProductReviews at "/product/{id}/reviews" {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}
