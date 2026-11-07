import k.auth.Session
import k.auth.SessionManager
import k.rpc.RpcOnlyError

public service AuthApi implements AuthProtocol {
    @Inject let sessions: SessionManager
    @Inject let session: Session
    @Inject let visitLog: VisitRepository

    rpc signIn(name: String) async throws RpcOnlyError {
        await sessions.signIn(userId: AccountId(name: name))
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

    rpc limited() async throws RpcOnlyError -> String {
        return "ok"
    }

    rpc startOAuth() async throws RpcOnlyError -> String {
        return await sessions.issueState()
    }

    rpc finishOAuth(state: String) async throws RpcOnlyError -> Bool {
        return await sessions.consumeState(state)
    }

    rpc visits() async throws RpcOnlyError -> Int {
        return await visitLog.total()
    }
}
