// `@Param` types that can't be read from a URL are reported at the type.
struct Point {
    let x: Int
}

enum Filter {
    case tag(String)
}

page Map {
    @Param let tags: List<String>
    @Param let center: Point
    @Param let filter: Filter
    @Param let since: Date
    @Param let zoom: Int

    view {
        text { "map" }
    }
}
