/// A component that fires `picked` with a payload.
public component Picker {
    @Emits let picked: Event<Int>

    view {
        button { "Pick" }
            .on(click: { picked.emit(3) })
    }
}
