component Counter {
    @State let count: Int = 0
    @Prop var label: String
    @State var fine: Int = 0

    view {
        text { label }
    }
}
