/// A page with a path parameter and an enum query parameter.
page Reading {
    @Param let id: Int
    @Param let units: Units = .metric

    view {
        text { if units == .imperial { "reading {id} in imperial" } else { "reading {id} in metric" } }
    }
}
