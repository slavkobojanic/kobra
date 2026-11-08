import k.auth.Session
import k.auth.SessionUser
import k.rpc.RpcOnlyError

/// The browser's side of the members protocol.
public service Members {
    @Inject let api: MembersProtocol
    @Inject let session: Session

    @Derived let user: SessionUser? = session.user

    func greeting() async throws RpcOnlyError -> String {
        return try await api.greeting()
    }

    func signOut() async throws RpcOnlyError {
        try await api.signOut()
    }
}
