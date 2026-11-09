import accounts.AuthService

/// The frame around every page: the app's name and who is signed in.
public layout AppLayout {
    @Inject let auth: AuthService

    /// The current page.
    @Slot let content: View

    @State var problem: String? = nil

    func signOut() async {
        problem = nil

        do {
            try await auth.signOut()

            Router.navigate(to: .signIn)
        } catch .rpcFailure {
            problem = "Couldn't reach Huddle, so you're still signed in. Try again."
        }
    }

    metadata {
        title("Huddle")
    }

    view {
        header {
            link(to: .lobby) { "Huddle" }
                .class(.brand)

            if let user = auth.user {
                nav {
                    text { user.id.name }

                    button { "Sign out" }
                        .on(click: signOut)
                }
                .class(.account)
                .label("Account")
            }
        }
        .class(.bar)

        if let problem {
            paragraph { problem }
                .role(.alert)
        }

        main {
            content
        }
    }

    style {
        .bar     { display: flex; justifyContent: spaceBetween; alignItems: center; padding: .md; }
        .brand   { fontWeight: bold; }
        .account { display: flex; gap: .sm; alignItems: center; }
    }
}
