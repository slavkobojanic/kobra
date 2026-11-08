// A protocol-level `RateLimit` lets guests through, so it doesn't narrow `session.user`.
import k.auth.AuthUserId
import k.auth.RateLimit
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol LimitedProtocol requires RateLimit(10.perMinute) {
    rpc hello() throws RpcOnlyError -> Bool
}

service LimitedApi implements LimitedProtocol {
    @Inject let session: Session

    rpc hello() async throws RpcOnlyError -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
