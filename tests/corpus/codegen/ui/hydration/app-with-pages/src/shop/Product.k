public page Product {
    @Param let id: Int

    view {
        main {
            heading(level: 1) { "Product {id}" }

            link(to: .home) { "Back" }
        }
    }
}
