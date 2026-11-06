// `loading` is contextual: as a field name after `let` it is an ordinary identifier, and `loading` used in a view is a name.
component T {
    @State var loading: Bool = true

    view {
        Spinner()
            .visible(loading)
    }
}
