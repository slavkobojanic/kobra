/// A named URL with a parameter after a static segment.
public page ProductReviews at "/product/{id}/reviews" {
    @Param let id: Int

    view {
        paragraph { "reviews of product {id}" }
    }
}
