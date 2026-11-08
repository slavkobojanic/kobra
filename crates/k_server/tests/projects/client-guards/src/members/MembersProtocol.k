import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// Signing in and out, and what only members may read.
public protocol MembersProtocol {
    rpc signIn(name: String, admin: Bool) throws RpcOnlyError
    rpc signOut() throws RpcOnlyError
    rpc greeting() throws RpcOnlyError -> String requires Authenticated
}
