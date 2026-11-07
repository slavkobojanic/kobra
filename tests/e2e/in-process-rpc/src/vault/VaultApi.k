import k.auth.SessionManager
import k.rpc.RpcOnlyError

public service VaultApi implements VaultProtocol {
    @Inject let sessions: SessionManager

    rpc signIn(name: String, admin: Bool) async throws RpcOnlyError {
        if admin {
            await sessions.signIn(userId: AccountId(name: name), roles: Set([StaffRole.admin]))
        } else {
            await sessions.signIn(userId: AccountId(name: name))
        }
    }

    rpc secret() async throws RpcOnlyError -> String {
        return "the vault"
    }

    rpc admin() async throws RpcOnlyError -> String {
        return "the admin vault"
    }

    rpc check(credentials: Credentials) async throws RpcOnlyError -> String {
        return "checked {credentials.name}"
    }

    rpc limited() async throws RpcOnlyError -> String {
        return "allowed"
    }
}
