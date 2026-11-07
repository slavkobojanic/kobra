// Props are getters: the parent passes `() => user.get()`, so a change flows into the child's
// bindings without re-running the child's factory (codegen.md §10).
component Greeting {
    @Prop let name: String

    view {
        text { "Hello, {name}" }
    }
}

component Home {
    @State var user: String = "Ada"

    view {
        Greeting(name: user)
    }
}
