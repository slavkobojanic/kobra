// `Session` is a client-safe service: components may inject it and read who is signed in.
import k.auth.Session

component AccountBadge {
    @Inject let session: Session

    view {
        if let user = session.user {
        //            ^^^^^^^^^^^^ type: k.auth.SessionUser?
            text { "signed in" }
        }
    }
}
