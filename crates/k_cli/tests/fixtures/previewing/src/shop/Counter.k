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

/// Why a setup fails.
enum SetupError implements Error {
    case broken
}

func failingSetup() throws SetupError {
    throw SetupError.broken
}

preview (value: Int = 1, label: String = "Items") {
    Counter(value: bind value, label: label)
}

preview "setup throws" (value: Int = 0) {
    try failingSetup()

    Counter(value: bind value)
}
