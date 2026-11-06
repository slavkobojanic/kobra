// Listening to the same event twice on one element is K3104, like setting any
// option twice.
component Counter {
    @State var count: Int = 0

    view {
        button { "Add" }
            .on(click: { count += 1 })
            .on(click: { count += 2 })
    }
}
