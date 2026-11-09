import k.ui.Router
import k.validation.Form
import k.validation.ValidationError

/// The page for making an account.
public page SignUp {
    @Inject let auth: AuthService
    @Form var credentials: Credentials = .empty
    @State var problem: String? = nil

    func submit() async throws ValidationError {
        problem = nil

        do {
            try await auth.signUp(credentials: credentials.value)

            Router.navigate(to: .home)
        } catch .emailTaken {
            problem = "There's already an account with this email address."
        } catch .closed {
            problem = "Shelf isn't taking new accounts."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure {
            problem = "Couldn't make the account. Check your connection and try again."
        }
    }

    metadata {
        title("Make an account · Shelf")
    }

    view {
        main {
            heading(level: 1) { "Make an account" }

            form {
                textField(value: bind credentials.email)
                    .label("Email")

                passwordField(value: bind credentials.password)
                    .label("Password")

                if let problem {
                    paragraph { problem }
                        .role(.alert)
                }

                button { "Make an account" }
                    .type(.submit)
            }
            .class(.form)
            .on(submit: credentials.handle(submit))

            paragraph {
                "Already have one? "

                link(to: .signIn) { "Sign in" }
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
