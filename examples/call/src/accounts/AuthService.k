import k.auth.Session
import k.auth.SessionUser
import k.rpc.RpcOnlyError

/// The browser's side of signing in and out.
public service AuthService {
    @Inject let api: AuthProtocol
    @Inject let session: Session

    /// The signed-in person, or `nil` for a guest.
    @Derived let user: SessionUser? = session.user

    /// Signs in under `name`.
    func signIn(name: String) async throws SignInError {
        try await api.signIn(name: name)
    }

    /// Signs out.
    func signOut() async throws RpcOnlyError {
        try await api.signOut()
    }
}
