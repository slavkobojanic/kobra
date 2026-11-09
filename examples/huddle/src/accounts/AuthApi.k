import k.auth.SessionManager
import k.rpc.RpcOnlyError

/// The server side of `AuthProtocol`.
public service AuthApi implements AuthProtocol {
    @Inject let sessions: SessionManager

    rpc signIn(name: String) async throws SignInError {
        let trimmed = name.trimmed()

        guard isValid(name: trimmed) else {
            throw SignInError.invalidName
        }

        await sessions.signIn(userId: AccountId(name: trimmed))
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }
}

/// `true` for 1 to 24 characters without spaces.
func isValid(name: String) -> Bool {
    return name.isNotEmpty and name.count <= 24 and not name.contains(" ")
}
