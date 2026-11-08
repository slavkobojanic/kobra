import k.auth.Authenticated
import k.auth.Role

/// The admin area.
public page Admin requires Authenticated, Role(.admin) {
    view {
        heading(level: 1) { "Admin" }
        link(to: .home) { "Home" }
    }
}
