// A `@State` initializer that reads a prop reads its getter once, in the child (codegen.md §10).
component Clicks {
    @Prop let start: Int = 0
    @State var count: Int = start

    view {
        text { "{count}" }
    }
}
