import k.auth.Authenticated
import k.auth.RateLimit
import k.auth.Role
import k.rpc.RpcOnlyError

public protocol VaultProtocol {
    rpc signIn(name: String, admin: Bool) throws RpcOnlyError
    rpc secret() throws RpcOnlyError -> String requires Authenticated
    rpc admin() throws RpcOnlyError -> String requires Authenticated, Role(.admin)
    rpc check(credentials: Credentials) throws RpcOnlyError -> String
    rpc limited() throws RpcOnlyError -> String requires RateLimit(2.perMinute)
}
