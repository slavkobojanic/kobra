// An `@Effect(on:)` dependency may be any reactive value of an injected
// service: its `@State`, its `@Derived` and its `var` fields, and paths
// into them. A service's plain `let` isn't reactive (K2001).
service Cart {
    @State var items: List<String> = []
    @Derived let count: Int = items.count
    @Derived let summary: Summary = Summary(count: items.count)
    let label: String = "Cart"
}

struct Summary {
    let count: Int
}

component Badge {
    @Inject let cart: Cart
    @State var bumps: Int = 0

    @Effect(on: [cart.count, cart.summary.count, cart.items])
    func bump() {
        bumps += 1
    }

    @Effect(on: [cart.label])
    func never() {
        bumps = 0
    }

    view {
        text { "{bumps}" }
    }
}
