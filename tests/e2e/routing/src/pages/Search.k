/// Only optional parameters: everything is in the query string.
public page Search {
    @Param let query: String = ""
    @Param let page: Int = 1
    @Param let tag: String? = nil

    view {
        paragraph { "search [{query}] page {page}" }

        if let tag {
            paragraph { "tag [{tag}]" }
        } else {
            paragraph { "no tag" }
        }

        link(to: .search.query(query).page(page + 1)) { "Next" }
    }
}
