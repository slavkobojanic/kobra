// A `@State` field is a signal on the client and a plain local in the server renderer, which has
// no signals (codegen.md §10, §11.2).
component Clicks {
    @State var count: Int = 0

    view {
        text { "{count}" }
    }
}
