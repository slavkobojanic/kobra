// `@Param` fields are `let`.
page Product {
    @Param var id: Int

    view {
        text { "{id}" }
    }
}
