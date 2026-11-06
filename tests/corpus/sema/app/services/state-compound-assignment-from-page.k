// A compound assignment to a service's field from a page is a write, too.
service Counter {
    @State var count: Int = 0
}

page Clicks {
    @Inject let counter: Counter

    func click() {
        counter.count += 1
    }

    view {
        button { "Click" }
            .on(click: click)
    }
}
