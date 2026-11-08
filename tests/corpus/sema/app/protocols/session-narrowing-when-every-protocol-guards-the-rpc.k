// A handler that serves an rpc of two protocols is narrowed when both guard it, one with a
// protocol-level clause and one on the rpc itself.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol VaultProtocol requires Authenticated {
    rpc note() throws RpcOnlyError -> Bool
}

protocol NotesProtocol {
    rpc note() throws RpcOnlyError -> Bool requires Authenticated
}

service VaultApi implements VaultProtocol, NotesProtocol {
    @Inject let session: Session

    rpc note() async throws RpcOnlyError -> Bool {
        return session.user.id == session.user.id
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser
    }
}
