// A `for` inside an event-handler lambda is code, not view content: no key.
component Totals {
    @Prop let values: List<Int>
    @State var total: Int = 0

    view {
        button { "Sum" }
            .on(click: {
                for value in values {
                    total += value
                }
            })
    }
}
