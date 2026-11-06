// A required `@Param` missing from the template is reported at the field; an optional one may stay in
// the query string.
page Product at "/product" {
    @Param let id: Int
    @Param let tab: String = "overview"

    view {
        text { "{id}" }
    }
}
