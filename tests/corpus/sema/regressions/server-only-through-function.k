// A plain function called from a component reaches the browser too: an entity
// constructed two calls away from a view is K4007, like one constructed in the
// component itself (sema.md §12.6).
entity Note {
    @Id let id: Int
    let text: String
}

func helper() -> Int {
    let n = Note(id: 1, text: "x")
    return n.id
}

func outer() -> Int {
    return helper()
}

component Card {
    view {
        text { "{outer()}" }
    }
}
