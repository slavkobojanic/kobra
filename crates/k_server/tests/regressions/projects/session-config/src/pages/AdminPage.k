import k.auth.Authenticated
import k.auth.Role

/// A page for admins.
public page AdminPage requires Authenticated, Role(.admin) {
    view {
        paragraph { "admin page" }
    }
}
