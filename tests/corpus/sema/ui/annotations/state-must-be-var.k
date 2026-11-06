// `@State` requires `var`, in a component and in a service alike.
component Counter {
    @State let count: Int = 0

    view {
        text { "{count}" }
    }
}

service Session {
    @State let userName: String = ""
    @State var visits: Int = 0
}
