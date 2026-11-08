// The server enforces the guards the protocol declares, not a `requires` clause on the
// implementation's rpc, so `requires Authenticated` written there doesn't narrow `session.user`.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol OpenProtocol {
    rpc hello() throws RpcOnlyError -> Bool
}

service OpenApi implements OpenProtocol {
    @Inject let session: Session

    rpc hello() async throws RpcOnlyError -> Bool requires Authenticated {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
