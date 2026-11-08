// A protocol-level `requires Authenticated` narrows `session.user` in the rpc bodies only; a
// private helper of the implementation takes the user explicitly, so it stays optional there.
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
        let user = session.user
        //         ^^^^^^^^^^^^ type: k.auth.SessionUser
        return user.id
    }

    private func signedIn() -> Bool {
        return session.user != nil
        //     ^^^^^^^^^^^^ type: k.auth.SessionUser?
    }
}
