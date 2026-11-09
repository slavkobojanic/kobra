/// A client-rendered component whose `view` panics.
@Rendering(.client)
public component Broken {
    @State var items: List<String> = []

    view {
        paragraph { items[2] }
    }

    loading {
        paragraph { "Loading the broken part…" }
    }
}
