// A handler must be a function (K2001 at the value).
component Counter {
    @State var count: Int = 0

    view {
        button { "Add" }
            .on(click: count)
    }
}
