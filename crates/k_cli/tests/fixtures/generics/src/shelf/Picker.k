/// A titled list of options of any type, each shown by `label`; clicking
/// one emits it. The type argument is inferred from `options`, or written
/// out: `Picker<Int>(title: …)`.
public component Picker<T> {
    @Prop let title: String
    @Prop let options: List<T>
    @Prop let label: (T) -> String

    @Emits let picked: Event<T>

    view {
        heading(level: 2) { title }

        list {
            for option in options key label(option) {
                item {
                    button { label(option) }
                        .type(.button)
                        .on(click: { picked.emit(option) })
                }
            }
        }
        .label(title)
    }
}
