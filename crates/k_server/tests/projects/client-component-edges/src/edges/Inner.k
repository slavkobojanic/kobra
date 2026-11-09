/// A client-rendered component used inside another one.
@Rendering(.client)
public component Inner {
    @Prop let label: String

    @State var note: String = ""

    view {
        paragraph { "Inner view {label}" }

        textField(value: bind note)
            .label("Inner note {label}")
    }

    loading {
        paragraph { "Inner loading {label}" }
    }
}
