// Handlers may be `async`; the element doesn't wait for them (04-components.md).
component Refresh {
    @State var count: Int = 0

    func reload() async {
        count += 1
    }

    view {
        button { "Reload" }
            .on(click: reload)
    }
}
