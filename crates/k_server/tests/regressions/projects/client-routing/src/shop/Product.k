/// A product page: `id` is an `Int`, so `id + 1` is arithmetic.
public page Product {
    @Param let id: Int
    @Param let sort: String = "new"

    view {
        main {
            heading(level: 1) { "next {id + 1}" }
            paragraph { "sort {sort}" }
        }
    }
}
