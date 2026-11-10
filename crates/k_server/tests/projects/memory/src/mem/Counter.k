/// A counter with derived text, a list and click handlers: what one mount
/// allocates in signals, effects, nodes and listeners.
public component Counter {
    /// The heading.
    @Prop let label: String

    @State var count: Int = 0
    @State var items: List<String> = ["one", "two", "three"]

    @Derived let summary: String = "Count: {count}"

    func add() {
        count = count + 1
        items = items + ["item {count}"]
    }

    func clear() {
        items = []
    }

    view {
        section {
            heading(level: 2) { label }

            paragraph { summary }

            list {
                for entry in items key entry {
                    item { entry }
                }
            }

            button { "Add" }
                .on(click: add)

            button { "Clear" }
                .on(click: clear)
        }
    }
}
