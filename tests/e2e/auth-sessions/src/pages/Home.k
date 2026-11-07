import k.auth.Session

/// A public page that reads the session.
public page Home at "/" {
    @Inject let session: Session

    view {
        if let user = session.user {
            paragraph { "home for {user.id.name}" }
        } else {
            paragraph { "home for a guest" }
        }
    }
}
