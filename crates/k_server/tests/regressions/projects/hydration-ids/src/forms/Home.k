import k.ui.ErrorBoundary

/// Labeled fields around an `ErrorBoundary` whose content takes a label id
/// and then panics (an index out of range), and rows over ranges.
public page Home {
    @State var name: String = ""
    @State var note: String = ""
    @State var other: String = ""
    @State var items: List<String> = ["a"]
    @State var index: Int = 5

    view {
        textField(value: bind name)
            .label("Name")

        ErrorBoundary {
            box {
                textField(value: bind note)
                    .label("Note")

                text { items[index] }
            }
        }
        .fallback {
            box {
                textField(value: bind other)
                    .label("Other")
            }
        }

        list {
            for i in 1...3 key i {
                item { "closed {i}" }
            }

            for i in 0..<2 key i {
                item { "open {i}" }
            }
        }
    }
}
