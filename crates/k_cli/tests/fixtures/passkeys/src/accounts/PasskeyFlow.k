import k.auth.PasskeyCreationOptions
import k.auth.PasskeyError
import k.auth.PasskeyRequestOptions
import k.auth.Passkeys
import k.rpc.RpcOnlyError

/// The browser's side: options fetched ahead, ceremonies from a click.
public service PasskeyFlow {
    @Inject let passkeys: Passkeys
    @Inject let auth: AuthProtocol

    /// The sign-up challenge, fetched before the click.
    @State var creation: PasskeyCreationOptions? = nil
    /// The sign-in challenge, fetched before the click.
    @State var request: PasskeyRequestOptions? = nil

    /// Fetches both challenges' options (the later one replaces the
    /// earlier in the session, so each ceremony fetches its own).
    func prepareSignUp() async {
        do {
            creation = try await auth.signUpOptions()
        } catch _ {
            creation = nil
        }
    }

    /// Fetches the sign-in options.
    func prepareSignIn() async {
        do {
            request = try await auth.signInOptions()
        } catch _ {
            request = nil
        }
    }

    /// Makes a passkey named `name` and signs up with it. The device's
    /// errors and the server's are caught in a `do` each: cases of two
    /// error enums in one `do` would be told apart by position only.
    func signUp(name: String) async -> String {
        guard let options = creation else {
            return "not prepared"
        }
        do {
            let registration = try await passkeys.register(options, name: name)
            return await send { try await auth.signUp(name: name, registration: registration) }
        } catch let refusal {
            return problem(refusal)
        }
    }

    /// Signs in with a passkey the device holds.
    func signIn() async -> String {
        guard let options = request else {
            return "not prepared"
        }
        do {
            let assertion = try await passkeys.signIn(options)
            return await send { try await auth.signIn(assertion: assertion) }
        } catch let refusal {
            return problem(refusal)
        }
    }

    /// The server's answer, or that it didn't answer.
    private func send(_ call: () async throws RpcOnlyError -> String) async -> String {
        do {
            return try await call()
        } catch _ {
            return "the server didn't answer"
        }
    }

    /// Signs out.
    func signOut() async {
        do {
            try await auth.signOut()
        } catch _ {
        }
    }
}

/// What the tests read for a ceremony the device ended.
func problem(_ refusal: PasskeyError) -> String {
    match refusal {
        case .unsupported { return "unsupported" }
        case .cancelled { return "cancelled" }
        case .alreadyRegistered { return "already registered" }
        case .failed { return "failed" }
    }
}
