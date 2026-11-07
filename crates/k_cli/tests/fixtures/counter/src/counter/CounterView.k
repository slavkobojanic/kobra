/// Shows the count, with a button that adds one.
public component CounterView {
    @Inject let counter: Counter

    @State private var problem: String? = nil

    view {
        heading(level: 2) { "Counter" }
        text { "Count: {counter.value}" }
        button { "Add" }
            .on(click: {
                do {
                    try counter.increment()
                } catch .atLimit {
                    problem = "That's the limit."
                }
            })
        if let p = problem {
            text { p }
                .role(.alert)
        }
    }
}
