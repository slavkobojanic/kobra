import admin.AdminService
import admin.SignInRequest
import k.validation.Form
import k.validation.ValidationError

/// The staff sign-in page.
///
/// `App.k` names this page with `@SignInPage`, so a visitor who opens a page
/// guarded by `k.auth.Authenticated` without a session is redirected here,
/// and comes back to that page after signing in.
public page SignIn {
    /// The page to return to after signing in.
    @Param let returnTo: Page = .admin

    @Inject let admin: AdminService

    /// The sign-in form. Both fields start blank: `@Form` keeps the text
    /// typed into each field and parses the email address only when the form
    /// validates.
    @Form var credentials: SignInRequest = .empty
    @State var problem: String? = nil

    /// Signs in and returns to the page that asked for it. Validation
    /// failures reported by the server are rethrown, so the form shows them
    /// under their fields.
    func submit() async throws ValidationError {
        problem = nil

        do {
            try await admin.signIn(request: credentials.value)

            Router.navigate(to: returnTo)
        } catch .invalidCredentials {
            problem = "That email and password don't match."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Wait a minute before trying again."
        } catch .rpcFailure(.offline) {
            problem = "You seem to be offline. Check your connection and try again."
        } catch .rpcFailure(.timeout) {
            problem = "The server didn't answer in time. Try again."
        }
    }

    metadata {
        title("Sign in · K Shop")
    }

    view {
        heading(level: 1) { "Staff sign-in" }

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
    }

    style {
        .form    { display: grid; gap: .md; maxWidth: 20rem; }
        .problem { color: .danger; }
    }
}
