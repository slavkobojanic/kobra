// A page in one module.
public page Product {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}
