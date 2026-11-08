import k.auth.Authenticated
import k.auth.Session

/// A signed-in member's page: its body reads `session.user` without a check.
public page Account requires Authenticated {
    @Inject let session: Session
    @Inject let members: Members

    @Load let greeting: String = try await members.greeting()

    /// Signs out; the router then leaves this page for the server.
    func signOut() async {
        do {
            try await members.signOut()
        } catch _ {
        }
    }

    loading {
        paragraph { "Loading…" }
    }

    view {
        heading(level: 1) { "Account of {session.user.id.name}" }
        paragraph { greeting }
        button { "Sign out" }
            .on(click: signOut)
        link(to: .admin) { "Admin" }
        link(to: .home) { "Home" }
    }
}
