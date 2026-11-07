import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.RpcOnlyError

/// Signing in and out, a rate-limited rpc and OAuth state.
public protocol AuthProtocol {
    rpc signIn(name: String) throws RpcOnlyError
    rpc signOut() throws RpcOnlyError
    rpc signOutEverywhere() throws RpcOnlyError requires Authenticated
    rpc status() throws RpcOnlyError -> String
    rpc limited() throws RpcOnlyError -> String requires RateLimit(4.perMinute)
    rpc startOAuth() throws RpcOnlyError -> String
    rpc finishOAuth(state: String) throws RpcOnlyError -> Bool
    rpc visits() throws RpcOnlyError -> Int
}
