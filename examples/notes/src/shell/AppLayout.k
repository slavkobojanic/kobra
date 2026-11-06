import accounts.AuthService

/// The frame around every page: the app name, and either account links or
/// the sign-out button.
///
/// `App.k` makes this the default layout, so pages don't declare it.
public layout AppLayout {
    @Inject let auth: AuthService

    /// The current page.
    @Slot let content: View

    /// Signs out and returns to the sign-in screen.
    func signOut() async {
        await auth.signOut()

        Router.navigate(to: .signIn)
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

        main { content }
            .class(.content)
    }

    style {
        .bar     { display: flex; alignItems: center; justifyContent: space-between; padding: .sm .lg; borderBottom: 1px solid .border; }
        .brand   { fontWeight: 600; color: .foreground; textDecoration: none; }
        .account { display: flex; alignItems: center; gap: .md; }
        .content { maxWidth: 48rem; margin: 0 auto; padding: .lg; }
    }
}
