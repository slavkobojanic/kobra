// Only `@Emits` fields are events: `.on(label: …)` on a component whose `label`
// is a prop is K3105.
component Counter {
    @Prop let label: String = "Count"

    view {
        text { label }
    }
}

component Home {
    @State var count: Int = 0

    view {
        Counter()
            .on(label: { count += 1 })
    }
}
