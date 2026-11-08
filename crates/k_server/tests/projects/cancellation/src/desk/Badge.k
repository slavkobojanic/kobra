import k.auth.Authenticated
import k.auth.Session
import k.observe.Log
import k.time.Clock

/// A guarded page that leaves work behind: listeners a service keeps, a
/// timer, and its own nodes, all of which read the narrowed `session.user`.
public page Badge requires Authenticated {
    @Inject let desk: Desk
    @Inject let bus: Bus
    @Inject let session: Session
    @Inject let clock: Clock

    func heard() async {
        Log.info("heard {session.user.id.name}")
    }

    func show(id: Int) async {
        Log.info("shown {id} for {session.user.id.name}")
    }

    @Effect
    func register() {
        bus.listen(heard)
        bus.listen({ Log.info("heard async {session.user.id.name}") })
        bus.listenPlain({ Log.info("heard plain {session.user.id.name}") })
    }

    view {
        paragraph { "badge" }

        button { "Who" }
            .on(click: { Log.info("who {session.user.id.name}") })

        button { "Remind" }
            .on(click: {
                let timer = clock.after(1.seconds) { Log.info("reminded {session.user.id.name}") }
            })

        button { "Show" }
            .on(click: { await desk.each(show) })

        button { "Fire" }
            .on(click: { await bus.fire() })

        button { "Sign out" }
            .on(click: {
                do {
                    try await desk.signOut()
                } catch _ {
                }
            })

        link(to: .board) { "Board" }
    }
}
