import k.rpc.RpcOnlyError

/// The browser's way to sign up, in and out.
public service AuthService {
    @Inject let api: AuthProtocol

    /// Makes an account and signs it in.
    func signUp(credentials: Credentials) async throws SignUpError {
        try await api.signUp(credentials: credentials)
    }

    /// Signs an account in.
    func signIn(credentials: Credentials) async throws SignInError {
        try await api.signIn(credentials: credentials)
    }

    /// Signs out.
    func signOut() async throws RpcOnlyError {
        try await api.signOut()
    }
}
