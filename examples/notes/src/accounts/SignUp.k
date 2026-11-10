import k.validation.Form
import k.validation.ValidationError

/// The account creation screen.
///
/// Field errors, including "already registered" from the email's `@Check`,
/// appear under their fields without any code here, because the form and
/// the rpc share `SignUpRequest`.
public page SignUp {
    @Inject let auth: AuthService

    @Form var request: SignUpRequest = .empty
    @State var problem: String? = nil

    /// Creates the account, then continues to the empty notes list. A
    /// validation failure from the server, such as the email's `@Check`, is
    /// passed back to the form, which shows it under its field.
    func submit() async throws ValidationError {
        problem = nil

        do {
            try await auth.signUp(request: request.value)

            Router.navigate(to: .notes)
        } catch .emailTaken {
            problem = "An account with this email was created a moment ago. Sign in instead."
        } catch .passkeyNotAccepted {
            problem = "That passkey wasn't accepted. Try again."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure(.offline), .rpcFailure(.timeout) {
            problem = "Couldn't reach K Notes. Check your connection and try again."
        }
    }

    metadata {
        title("Create an account · K Notes")
    }

    view {
        heading(level: 1) { "Create an account" }

        form {
            textField(value: bind request.email)
                .label("Email")

            passwordField(value: bind request.password)
                .label("Password")

            passwordField(value: bind request.confirmPassword)
                .label("Confirm password")

            if let problem {
                paragraph { problem }
                    .class(.problem)
                    .role(.alert)
            }

            button { "Create account" }
                .type(.submit)
                .disabled(request.isSubmitting)
        }
        .class(.form)
        .on(submit: request.handle(submit))

        PasskeySignUp()

        paragraph {
            "Already have an account? "

            link(to: .signIn) { "Sign in" }
        }
    }

    style {
        .form    { display: grid; gap: .md; maxWidth: 24rem; }
        .problem { color: .danger; }
    }
}
