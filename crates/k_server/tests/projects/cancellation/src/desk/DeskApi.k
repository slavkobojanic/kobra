import k.auth.SessionManager
import k.rpc.RpcOnlyError

/// The server side of `DeskProtocol`.
public service DeskApi implements DeskProtocol {
    @Inject let sessions: SessionManager

    rpc save(text: String) async throws RpcOnlyError -> String {
        return text
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }

    rpc lookup(id: Int) async throws RpcOnlyError -> String {
        return "name {id}"
    }
}
