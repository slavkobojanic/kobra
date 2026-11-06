// In the body of an rpc whose protocol `requires Authenticated`, `session.user` is not optional.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol ProfileProtocol requires Authenticated {
    rpc me() throws RpcOnlyError -> AccountId
}

service ProfileApi implements ProfileProtocol {
    @Inject let session: Session

    rpc me() async throws RpcOnlyError -> AccountId {
        return session.user.id
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser
    }
}
