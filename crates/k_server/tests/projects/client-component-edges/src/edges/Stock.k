func stock(of name: String) async -> Int {
    return name.count * 10
}

/// A server-rendered component with its own `@Load` and a labelled control.
public component Stock {
    @Prop let name: String

    @Load let count: Int = await stock(of: name)

    @State var note: String = ""

    view {
        paragraph { "{name} in stock: {count}" }

        textField(value: bind note)
            .label("{name} note")
    }

    loading {
        paragraph { "Counting {name}…" }
    }
}
