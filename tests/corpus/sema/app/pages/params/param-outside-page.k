// `@Param` applies only to page fields.
component Card {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}
