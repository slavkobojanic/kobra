// A plain `var` field is ordinary data: no signal, no binding effect (03-reactivity.md).
component Tally {
    var clicks: Int = 0

    func click() {
        clicks += 1
    }

    view {
        button { "Click" }
            .on(click: click)
    }
}
