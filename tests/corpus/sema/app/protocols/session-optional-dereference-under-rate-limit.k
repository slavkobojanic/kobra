// Reading a member of `session.user` without unwrapping it, in an rpc guests can reach, is K2017.
import k.auth.AuthUserId
import k.auth.RateLimit
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol GuestProtocol {
    rpc me() throws RpcOnlyError -> AccountId requires RateLimit(10.perMinute)
}

service GuestApi implements GuestProtocol {
    @Inject let session: Session

    rpc me() async throws RpcOnlyError -> AccountId {
        return session.user.id
    }
}
