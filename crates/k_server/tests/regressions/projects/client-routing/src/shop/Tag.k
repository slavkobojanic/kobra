/// A tag page with only `String` parameters.
public page Tag {
    @Param let name: String
    @Param let sort: String = "new"

    view {
        main {
            heading(level: 1) { "tag {name}" }
            paragraph { "sort {sort}" }
        }
    }
}
