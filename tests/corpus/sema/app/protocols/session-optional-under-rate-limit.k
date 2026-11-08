// A guest can reach an rpc guarded only by `RateLimit`, so `session.user` stays optional there,
// in a sibling rpc with no guard, and in a private helper of the same service.
import k.auth.AuthUserId
import k.auth.RateLimit
import k.auth.Session
import k.crypto.Uuid
import k.rpc.RpcOnlyError

struct AccountId implements AuthUserId {
    let value: Uuid
}

protocol GuestProtocol {
    rpc hello() throws RpcOnlyError -> Bool requires RateLimit(10.perMinute)
    rpc ping() throws RpcOnlyError -> Bool
}

service GuestApi implements GuestProtocol {
    @Inject let session: Session

    rpc hello() async throws RpcOnlyError -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }

    rpc ping() async throws RpcOnlyError -> Bool {
        return signedIn()
    }

    private func signedIn() -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
