func basketTotal() async -> Int {
    return 7
}

/// A client-rendered component with its own `@Load` and a labelled control.
@Rendering(.client)
public component Basket {
    @Load let total: Int = await basketTotal()

    @State var note: String = ""

    view {
        paragraph { "Basket total: {total}" }

        textField(value: bind note)
            .label("Basket note")
    }

    loading {
        paragraph { "Loading the basket…" }
    }
}
