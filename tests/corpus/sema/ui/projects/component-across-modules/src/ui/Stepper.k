/// A number with − and + buttons, used from another module.
public component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Emits let changed: Event<Int>

    func add() {
        value += step
        changed.emit(value)
    }

    view {
        button { "+" }
            .on(click: add)
    }
}
