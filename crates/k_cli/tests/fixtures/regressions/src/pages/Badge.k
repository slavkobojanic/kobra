/// A name shown with `if let`: the branch stays while the name is set,
/// and its text follows a rename (an `if let` binding used to be read once
/// when the branch rendered, so a new non-nil value never showed).
public component Badge {
    @State var name: String? = "Ada"

    view {
        if let name {
            text { "Hello, {name}" }

            button { "Rename" }
                .on(click: { self.name = "Grace" })
        } else {
            text { "Nobody" }
        }

        button { "Clear" }
            .on(click: { self.name = nil })
    }
}
