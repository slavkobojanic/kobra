// A shared service injected into a component runs in the browser; an entity used
// in one of its methods is server-only code reaching the client: K4007.
entity Note {
    @Id let id: Int
    let text: String
}

service Cart {
    func count() -> Int {
        let n = Note(id: 1, text: "x")
        return n.id
    }
}

component Card {
    @Inject let cart: Cart

    view {
        text { "{cart.count()}" }
    }
}
