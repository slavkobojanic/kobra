// An `Event<Void>` is fired with `emit()` and handled by a lambda without parameters
// (04-components.md).
component Dialog {
    @Emits let closed: Event<Void>

    view {
        button { "Close" }
            .on(click: { closed.emit() })
    }
}

component Settings {
    @State var isOpen: Bool = true

    view {
        Dialog()
            .on(closed: { isOpen = false })
    }
}
