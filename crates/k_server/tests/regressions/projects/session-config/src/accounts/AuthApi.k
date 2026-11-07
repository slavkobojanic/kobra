import k.auth.Session
import k.auth.SessionManager
import k.rpc.RpcOnlyError

/// The server side of `AuthProtocol`, with two hard-coded accounts.
public service AuthApi implements AuthProtocol {
    @Inject let sessions: SessionManager
    @Inject let session: Session

    rpc signIn(credentials: Credentials) async throws SignInError {
        guard credentials.password == "correct horse" else {
            throw SignInError.invalidCredentials
        }

        if credentials.name == "root" {
            await sessions.signIn(userId: AccountId(name: "root"), roles: Set([StaffRole.admin]))
            return
        }

        guard credentials.name == "ada" else {
            throw SignInError.invalidCredentials
        }

        await sessions.signIn(userId: AccountId(name: "ada"))
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }

    rpc signOutEverywhere() async throws RpcOnlyError {
        await sessions.endAll(userId: session.user.id)
    }

    rpc status() async throws RpcOnlyError -> String {
        if let user = session.user {
            return "signed in as {user.id.name}"
        }

        return "guest"
    }

    rpc whoami() async throws RpcOnlyError -> String {
        return session.user.id.name
    }

    rpc adminReport() async throws RpcOnlyError -> String {
        return "report for {session.user.id.name}"
    }
}
