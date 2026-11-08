// One handler serves an rpc of every implemented protocol that declares it, so it is narrowed
// only when each of them guards it with `Authenticated`: here `OpenProtocol.note` lets guests
// reach the same body that `VaultProtocol.note` guards.
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

protocol OpenProtocol {
    rpc note() throws RpcOnlyError -> Bool
}

service VaultApi implements VaultProtocol, OpenProtocol {
    @Inject let session: Session

    rpc note() async throws RpcOnlyError -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
