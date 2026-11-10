import k.auth.PasskeyCreationOptions
import k.auth.PasskeyRequestOptions
import k.auth.Passkeys

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
    /// errors and the server's are caught in one `do`: each clause matches
    /// its own enum's case, though `PasskeyError.unsupported` and
    /// `RpcOnlyError.rpcFailure` are both case 0.
    func signUp(name: String) async -> String {
        guard let options = creation else {
            return "not prepared"
        }
        do {
            let registration = try await passkeys.register(options, name: name)
            return try await auth.signUp(name: name, registration: registration)
        } catch .rpcFailure {
            return "the server didn't answer"
        } catch .unsupported {
            return "unsupported"
        } catch .cancelled {
            return "cancelled"
        } catch .alreadyRegistered {
            return "already registered"
        } catch .failed {
            return "failed"
        }
    }

    /// Signs in with a passkey the device holds.
    func signIn() async -> String {
        guard let options = request else {
            return "not prepared"
        }
        do {
            let assertion = try await passkeys.signIn(options)
            return try await auth.signIn(assertion: assertion)
        } catch .rpcFailure {
            return "the server didn't answer"
        } catch .unsupported {
            return "unsupported"
        } catch .cancelled {
            return "cancelled"
        } catch .alreadyRegistered {
            return "already registered"
        } catch .failed {
            return "failed"
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
