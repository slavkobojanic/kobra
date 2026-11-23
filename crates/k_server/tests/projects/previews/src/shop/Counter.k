/// A counter with a two-way value.
public component Counter {
    @Binding var value: Int
    @Prop let label: String = "Count"

    view {
        text { "{label}: {value}" }

        button { "Add" }
            .on(click: { value += 1 })
    }
}

preview (value: Int = 1, label: String = "Items") {
    Counter(value: bind value, label: label)
}
