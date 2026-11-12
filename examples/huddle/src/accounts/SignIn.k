/// The sign-in screen: pick a name.
///
/// `App.k` names this page with `@SignInPage`, so a visitor who opens a room
/// without a session is sent here, and comes back afterwards.
public page SignIn {
    /// The page to continue to after signing in.
    @Param let returnTo: Page = .lobby

    @Inject let auth: AuthService

    @State var name: String = ""
    @State var problem: String? = nil

    func submit() async {
        problem = nil

        do {
            try await auth.signIn(name: name)

            Router.navigate(to: returnTo)
        } catch .invalidName {
            problem = "Use 1 to 24 characters, without spaces."
        } catch .rpcFailure {
            problem = "Couldn't reach Huddle. Check your connection and try again."
        }
    }

    metadata {
        title("Sign in · Huddle")
    }

    view {
        heading(level: 1) { "Sign in" }

        form {
            textField(value: bind name)
                .label("Your name")

            if let problem {
                paragraph { problem }
                    .class(.problem)
                    .role(.alert)
            }

            button { "Join" }
                .type(.submit)
        }
        .class(.form)
        .on(submit: submit)
    }

    style {
        .form { display: grid; gap: .md; maxWidth: 24rem; }
        .problem { color: .danger; }
    }
}
