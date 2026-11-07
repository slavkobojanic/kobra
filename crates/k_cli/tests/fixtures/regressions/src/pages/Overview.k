/// A page whose only parameter is optional: `Page.overview` is a value.
page Overview {
    @Param let units: Units = .metric

    view {
        text { "overview" }
    }
}
