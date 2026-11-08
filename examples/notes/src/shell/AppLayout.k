import accounts.AuthService

/// The frame around every page: the app name, and either account links or
/// the sign-out button.
///
/// `App.k` makes this the default layout, so pages don't declare it.
public layout AppLayout {
    @Inject let auth: AuthService

    /// The current page.
    @Slot let content: View

    /// Why the last sign-out failed, for the alert.
    @State var problem: String? = nil

    /// Signs out and returns to the sign-in screen. When the server can't be
    /// reached, the session is still active, so the page says so instead.
    func signOut() async {
        problem = nil

        do {
            try await auth.signOut()

            Router.navigate(to: .signIn)
        } catch .rpcFailure {
            problem = "Couldn't reach K Notes, so you're still signed in. Try again."
        }
    }

    metadata {
        title("K Notes")
    }

    view {
        header {
            link(to: .notes) { "K Notes" }
                .class(.brand)

            nav {
                if auth.user == nil {
                    link(to: .signIn) { "Sign in" }

                    link(to: .signUp) { "Create account" }
                } else {
                    button { "Sign out" }
                        .on(click: signOut)
                }
            }
            .class(.account)
            .label("Account")
        }
        .class(.bar)

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }

        main { content }
            .class(.content)
    }

    style {
        .bar     { display: flex; alignItems: center; justifyContent: spaceBetween; padding: .sm .lg; borderBottom: 1px solid .border; }
        .brand   { fontWeight: 600; color: .foreground; textDecoration: none; }
        .account { display: flex; alignItems: center; gap: .md; }
        .content { maxWidth: 48rem; margin: 0 auto; padding: .lg; }
        .problem { margin: .sm .lg 0; color: .danger; }
    }
}
