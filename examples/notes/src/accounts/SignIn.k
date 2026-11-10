import k.validation.Form
import k.validation.ValidationError

/// The sign-in screen.
///
/// `App.k` names this page with `@SignInPage`, so a visitor who opens a page
/// guarded by `Authenticated` without a session is sent here, and comes back
/// to that page after signing in.
public page SignIn {
    /// The page to continue to after signing in.
    @Param let returnTo: Page = .notes

    @Inject let auth: AuthService

    /// Starts with both fields blank. The form keeps what is typed, and
    /// parses the email when it validates.
    @Form var credentials: Credentials = .empty
    @State var problem: String? = nil

    /// Signs in, then continues to `returnTo`. A validation failure from the
    /// server is passed back to the form, which shows it under its fields.
    ///
    /// `handle` runs this only once the form is valid, so this is the one
    /// place `credentials.value`, the parsed `Credentials`, can be read.
    func submit() async throws ValidationError {
        problem = nil

        do {
            try await auth.signIn(credentials: credentials.value)

            Router.navigate(to: returnTo)
        } catch .invalidCredentials, .passkeyNotAccepted {
            problem = "That email and password don't match an account."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure(.offline), .rpcFailure(.timeout) {
            problem = "Couldn't reach K Notes. Check your connection and try again."
        }
    }

    metadata {
        title("Sign in · K Notes")
    }

    view {
        heading(level: 1) { "Sign in" }

        form {
            textField(value: bind credentials.email)
                .label("Email")

            passwordField(value: bind credentials.password)
                .label("Password")

            if let problem {
                paragraph { problem }
                    .class(.problem)
                    .role(.alert)
            }

            button { "Sign in" }
                .type(.submit)
                .disabled(credentials.isSubmitting)
        }
        .class(.form)
        .on(submit: credentials.handle(submit))

        PasskeySignIn(returnTo: returnTo)

        paragraph {
            "New here? "

            link(to: .signUp) { "Create an account" }
        }
    }

    style {
        .form    { display: grid; gap: .md; maxWidth: 24rem; }
        .problem { color: .danger; }
    }
}
