/// A client-rendered component with another one in its `loading` (which
/// the server renders) and in its `view`.
@Rendering(.client)
public component Outer {
    view {
        paragraph { "Outer view" }

        Inner(label: "in view")
    }

    loading {
        paragraph { "Outer loading" }

        Inner(label: "in loading")
    }
}
