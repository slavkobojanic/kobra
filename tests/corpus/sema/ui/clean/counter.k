// The `Counter` of 04-components.md: props with and without defaults, an event,
// state initialized from a prop, a handler, a view and a style block.
theme Tokens {
    fontSize lg: FontSize = 1.25rem
}

component Counter {
    @Prop let label: String
    @Prop let start: Int = 0
    @Emits let changed: Event<Int>

    @State var count: Int = start

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        box {
            text { "{label}: {count}" }
                .class(.label)

            button { "Add" }
                .on(click: increment)
        }
    }

    style {
        .label { fontSize: .lg; }
    }
}
