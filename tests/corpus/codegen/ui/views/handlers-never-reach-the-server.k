// The server renderer has no event handlers: nothing listens during SSR (codegen.md §11.2).
component Counter {
    @State var count: Int = 0

    view {
        button { "{count}" }
            .on(click: { count += 1 })
    }
}
