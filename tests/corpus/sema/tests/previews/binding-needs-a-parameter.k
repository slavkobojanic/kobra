// A literal or a `let` can't be bound: a fixed-state preview of a
// `@Binding` component takes a parameter.
component Toggle {
    @Binding var on: Bool

    view {
        "toggle"
    }
}

preview "a let" {
    let on = true
    Toggle(on: bind on)
}

preview "a parameter" (on: Bool = true) {
    Toggle(on: bind on)
}

preview "a typo" (on: Bool = true) {
    Toggle(on: bind off)
}
