// A field named `loading` and its use as an `if` condition inside a view.
component T {
    @State var loading: Bool = false

    view {
        if loading {
            Spinner()
        }
    }
}
