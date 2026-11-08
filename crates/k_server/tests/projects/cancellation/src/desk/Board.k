/// An open page that fires the listeners another page left in `Bus`, and
/// signs out without leaving.
public page Board {
    @Inject let desk: Desk
    @Inject let bus: Bus

    view {
        paragraph { "board" }

        button { "Fire" }
            .on(click: { await bus.fire() })

        button { "Sign out" }
            .on(click: {
                do {
                    try await desk.signOut()
                } catch _ {
                }
            })
    }
}
