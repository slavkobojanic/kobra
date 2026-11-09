// A client-rendered component: its view, field initializers and loads run only in the browser.
let skeletonRows: Int = 3

/// Not `Codable`: a load of a client-rendered component is never serialized.
struct Snapshot {
    let count: Int
}

func snapshot(count: Int) async -> Snapshot {
    return Snapshot(count: count)
}

@Rendering(.client)
public component TodoList {
    @Prop let title: String

    @Slot let footer: View

    @Inject let todos: TodoService
    let count: Int = todos.items.count

    @Load let snap: Snapshot = await snapshot(count: todos.items.count)

    view {
        heading(level: 2) { "{title}: {todos.items.count} ({snap.count})" }
        for item in todos.items key item {
            TodoRow(text: item)
        }
        footer
    }

    loading {
        paragraph { "Loading {title}…" }
        for row in 0..<skeletonRows key row {
            Skeleton(row: row)
        }
    }
}
