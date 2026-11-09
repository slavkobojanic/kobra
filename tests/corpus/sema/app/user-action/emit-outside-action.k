// A component may fire an event declared `@UserAction` only from a user's action itself, since its
// parent's handler may then open a prompt; firing it from an effect is K4030. `@UserAction` applies
// to component events and interface functions, and nowhere else (K3002).
component Prompting {
    @UserAction
    @Emits let pressed: Event<Void>

    @UserAction
    @Prop let label: String

    @State var count: Int = 0

    @Effect
    func announce() {
        pressed.emit()
    }

    @UserAction
    func press() {
        pressed.emit()
    }

    view {
        button { label }
            .on(click: press)
    }
}
