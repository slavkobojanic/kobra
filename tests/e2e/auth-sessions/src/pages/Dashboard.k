import k.auth.Authenticated
import k.auth.Session

/// A page for signed-in users. `session.user` is narrowed here.
public page Dashboard requires Authenticated {
    @Param let tab: String = "main"

    @Inject let session: Session

    view {
        paragraph { "dashboard {tab} for {session.user.id.name}" }
    }
}
