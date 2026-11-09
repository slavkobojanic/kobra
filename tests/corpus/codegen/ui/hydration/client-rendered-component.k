// A client-rendered component (47-client-rendered-components.md): the server renders only its
// `loading` block, framed as a block range, and nothing of its view, state or loads; the browser
// renders `loading` while hydrating, then `TodoList$view` after the pass (`$k.clientReady()`).
// Its loads never hydrate from `k-data`.
func count(of title: String) async -> Int {
    return title.count
}

@Rendering(.client)
component TodoList {
    @Prop let title: String
    @Prop let rows: Int = 3

    @State var shown: Int = 0

    @Load let total: Int = await count(of: title)

    view {
        heading(level: 2) { "{title}: {shown} of {total}" }
    }

    loading {
        paragraph { "Loading {title} ({rows} rows)…" }
    }
}
