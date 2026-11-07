// `count += 1` on a `@State` Int is one signal write of a checked sum: test builds print like
// development builds, so `Int` overflow is checked (codegen.md §5, §10).
component Clicks {
    @State var count: Int = 0

    func increment() {
        count += 1
    }

    view {
        button { "{count}" }
            .on(click: increment)
    }
}
