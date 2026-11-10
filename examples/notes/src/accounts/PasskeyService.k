import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyError
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.auth.Passkeys
import k.text.EmailAddress

/// The browser's side of passkeys: the server's options, fetched before
/// the click, and the two ceremonies, run from it.
///
/// The device's passkey sheet may only open from a click (`register` and
/// `signIn` are `@UserAction`), so `signUp(email:)` and `signIn()` call
/// them before any other `await`, with options a page's effect fetched
/// earlier with `prepareSignUp()` or `prepareSignIn()`. A challenge is
/// good for one attempt, so both fetch the next one whatever happened.
public service PasskeyService {
    @Inject let passkeys: Passkeys
    @Inject let api: AuthProtocol

    /// Whether to offer passkeys: `false` where the browser has no
    /// WebAuthn, and while the server renders.
    @Derived let isSupported: Bool = passkeys.isSupported

    /// The sign-up challenge, once fetched.
    @State var signUpOptions: PasskeyCreationOptions? = nil

    /// The sign-in challenge, once fetched.
    @State var signInOptions: PasskeyRequestOptions? = nil

    /// Fetches the options for signing up with a passkey.
    func prepareSignUp() async {
        do {
            signUpOptions = try await api.passkeySignUpOptions()
        } catch .rpcFailure {
            signUpOptions = nil
        }
    }

    /// Fetches the options for signing in with a passkey.
    func prepareSignIn() async {
        do {
            signInOptions = try await api.passkeySignInOptions()
        } catch .rpcFailure {
            signInOptions = nil
        }
    }

    /// Makes a passkey labelled with `email` and creates its account.
    ///
    /// - Parameter email: the address typed.
    /// - Returns: `nil` once signed in, else what to tell the person.
    func signUp(email: EmailAddress) async -> String? {
        guard let options = signUpOptions else {
            return "Passkeys aren't ready yet. Try again in a moment."
        }

        // The device's errors and the server's are caught apart: one `do`
        // per error type.
        var problem: String? = nil
        do {
            let registration = try await passkeys.register(options, name: email.address)
            problem = await createAccount(email: email, registration: registration)
        } catch let refusal {
            problem = deviceProblem(refusal, making: true)
        }

        if problem != nil {
            await prepareSignUp()
        }
        return problem
    }

    /// Signs in with a passkey the device holds for K Notes.
    ///
    /// - Returns: `nil` once signed in, else what to tell the person.
    func signIn() async -> String? {
        guard let options = signInOptions else {
            return "Passkeys aren't ready yet. Try again in a moment."
        }

        var problem: String? = nil
        do {
            let assertion = try await passkeys.signIn(options)
            problem = await signInWith(assertion: assertion)
        } catch let refusal {
            problem = deviceProblem(refusal, making: false)
        }

        if problem != nil {
            await prepareSignIn()
        }
        return problem
    }

    /// Sends a new passkey to the server: `nil` once its account exists
    /// and is signed in.
    private func createAccount(email: EmailAddress, registration: PasskeyRegistration) async -> String? {
        do {
            try await api.signUpWithPasskey(email: email, registration: registration)
            return nil
        } catch .emailTaken {
            return "An account with this email already exists. Sign in instead."
        } catch .passkeyNotAccepted {
            return "That passkey wasn't accepted. Try again."
        } catch .rpcFailure(.rejected) {
            return "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure {
            return "Couldn't reach K Notes. Check your connection and try again."
        }
    }

    /// Sends a passkey's signature to the server: `nil` once signed in.
    private func signInWith(assertion: PasskeyAssertion) async -> String? {
        do {
            try await api.signInWithPasskey(assertion: assertion)
            return nil
        } catch .invalidCredentials, .passkeyNotAccepted {
            return "That passkey wasn't accepted."
        } catch .rpcFailure(.rejected) {
            return "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure {
            return "Couldn't reach K Notes. Check your connection and try again."
        }
    }
}

/// What to tell the person when the device's sheet ended without a
/// passkey.
func deviceProblem(_ refusal: PasskeyError, making: Bool) -> String {
    match refusal {
        case .cancelled {
            return if making { "No passkey was made. Try again when you're ready." } else { "No passkey was used. If this device has none for K Notes, sign in with your password." }
        }
        case .alreadyRegistered { return "This device already has a passkey for this account." }
        case .unsupported, .failed {
            return if making { "This browser couldn't make a passkey here." } else { "This browser couldn't use a passkey here." }
        }
    }
}
