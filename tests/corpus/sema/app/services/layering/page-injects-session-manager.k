// `SessionManager` is injectable only into server-side services; pages read `Session` instead.
import k.auth.SessionManager

page SignOut {
    @Inject let sessions: SessionManager

    view {
        text { "bye" }
    }
}
