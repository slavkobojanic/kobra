// A chained modifier with both arguments and a trailing block.
component T {
    view {
        Icon(name: "info")
            .tooltip(placement: .top) { "More information" }
    }
}
