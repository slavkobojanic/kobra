// An `@Emits` field is an `Event<T>`; another type is K3005 at the type, like a
// slot that isn't a `View`.
component Counter {
    @Emits let changed: Int
    @Emits let reset: Event<Void>

    view {
        button { "Reset" }
            .on(click: { reset.emit() })
    }
}
