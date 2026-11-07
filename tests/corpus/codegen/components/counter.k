// path: src/corpus/Counter.k
// The Counter of docs/compiler/architecture.md §7.2: a client factory over signals with static
// template cloning, and a server renderer that escapes every dynamic value
// (docs/compiler/codegen.md §10, §11).
public component Counter {
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

            button { "Add" }
                .on(click: increment)
        }
    }
}
