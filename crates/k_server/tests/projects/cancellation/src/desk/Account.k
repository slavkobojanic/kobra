import k.auth.Authenticated
import k.auth.Session
import k.observe.Log

/// A guarded page whose inline handler reads the narrowed `session.user`
/// after awaiting an rpc.
public page Account requires Authenticated {
    @Inject let desk: Desk
    @Inject let session: Session

    view {
        paragraph { "account" }

        button { "Save" }
            .on(click: {
                do {
                    let saved = try await desk.save(text: "draft")
                    Log.info("saved {saved} for {session.user.id.name}")
                } catch _ {
                    Log.info("save failed for {session.user.id.name}")
                }
            })

        button { "Sign out" }
            .on(click: {
                do {
                    try await desk.signOut()
                } catch _ {
                }
            })
    }
}
