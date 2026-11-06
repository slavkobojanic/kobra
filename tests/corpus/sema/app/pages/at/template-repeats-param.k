// A template mentions each parameter exactly once.
page Product at "/p/{id}/{id}" {
    @Param let id: Int

    view {
        text { "{id}" }
    }
}
