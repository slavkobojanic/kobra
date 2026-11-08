// An rpc whose own `requires` includes `Authenticated` (next to other guards) narrows
// `session.user`; a sibling rpc guarded only by `RateLimit`, and a private helper, don't.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.RateLimit
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol MixedProtocol {
    rpc me() throws RpcOnlyError -> AccountId requires RateLimit(10.perMinute), Authenticated
    rpc hello() throws RpcOnlyError -> Bool requires RateLimit(10.perMinute)
}

service MixedApi implements MixedProtocol {
    @Inject let session: Session

    rpc me() async throws RpcOnlyError -> AccountId {
        return session.user.id
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser
    }

    rpc hello() async throws RpcOnlyError -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }

    private func helper() -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
