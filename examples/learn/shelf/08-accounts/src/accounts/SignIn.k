import k.ui.Router
import k.validation.Form
import k.validation.ValidationError

/// The sign-in page. A page that needs someone signed in sends everyone
/// else here, and `returnTo` takes them back afterwards.
public page SignIn {
    /// The page to go to after signing in.
    @Param let returnTo: Page = .home

    @Inject let auth: AuthService
    @Form var credentials: Credentials = .empty
    @State var problem: String? = nil

    func submit() async throws ValidationError {
        problem = nil

        do {
            try await auth.signIn(credentials: credentials.value)

            Router.navigate(to: returnTo)
        } catch .invalidCredentials {
            problem = "That email address and password don't match an account."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure {
            problem = "Couldn't sign in. Check your connection and try again."
        }
    }

    metadata {
        title("Sign in · Shelf")
    }

    view {
        main {
            heading(level: 1) { "Sign in" }

            form {
                textField(value: bind credentials.email)
                    .label("Email")

                passwordField(value: bind credentials.password)
                    .label("Password")

                if let problem {
                    paragraph { problem }
                        .role(.alert)
                }

                button { "Sign in" }
                    .type(.submit)
            }
            .class(.form)
            .on(submit: credentials.handle(submit))

            paragraph {
                "New to Shelf? "

                link(to: .signUp) { "Make an account" }
            }
        }
        .class(.page)
    }

    style {
        .page {
            maxWidth: 24rem;
            margin: 0 auto;
            padding: .lg;
            font: .body;
        }

        .form {
            display: grid;
            gap: .sm;
        }
    }
}
