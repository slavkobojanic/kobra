import k.auth.PasskeyCreationOptions
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

        // The device's errors (`PasskeyError`) and the server's
        // (`SignUpError`) are caught in one `do`, each case by its own enum.
        var problem: String? = nil
        do {
            let registration = try await passkeys.register(options, name: email.address)
            try await api.signUpWithPasskey(email: email, registration: registration)
        } catch .cancelled {
            problem = "No passkey was made. Try again when you're ready."
        } catch .alreadyRegistered {
            problem = "This device already has a passkey for this account."
        } catch .unsupported, .failed {
            problem = "This browser couldn't make a passkey here."
        } catch .emailTaken {
            problem = "An account with this email already exists. Sign in instead."
        } catch .passkeyNotAccepted {
            problem = "That passkey wasn't accepted. Try again."
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure {
            problem = "Couldn't reach K Notes. Check your connection and try again."
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
            try await api.signInWithPasskey(assertion: assertion)
        } catch .cancelled {
            problem = "No passkey was used. If this device has none for K Notes, sign in with your password."
        } catch .alreadyRegistered {
            problem = "This device already has a passkey for this account."
        } catch .unsupported, .failed {
            problem = "This browser couldn't use a passkey here."
        } catch .invalidCredentials, .passkeyNotAccepted {
            problem = "That passkey wasn't accepted."
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Wait a minute, then try again."
        } catch .rpcFailure {
            problem = "Couldn't reach K Notes. Check your connection and try again."
        }

        if problem != nil {
            await prepareSignIn()
        }
        return problem
    }
}
